// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The six guest core threads (ADR-0017 B2 scope): created when the process runs, parked
//! while their core has no ready thread, joined before [`run_cores`] returns.
//!
//! The kernel sits behind one lock (ADR-0032 §4). A core holds it only at a safe point (an
//! SVC, the end of a quantum, an interrupt) and runs guest code with it released. Every
//! change of a thread's status, priority or core is followed, under the same lock hold, by one
//! reselection pass: each core that runs a thread other than the one it should run gets its
//! interrupt flag set, which `run_interruptible` sees at the next block boundary, and parked
//! cores wake to look again. A thread's state is either in the table or held by the one core
//! it runs on, and moves between the two only under the lock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use lince_cpu_a64::{Exit, State, run_interruptible};
use lince_mem::AddressSpace;
use parking_lot::{Condvar, Mutex, MutexGuard};

use crate::error::{KernelError, RunError};
use crate::kernel::{Current, Kernel, Outcome, Regs};
use crate::thread::CORE_COUNT;

/// Instructions in one quantum when [`run_cores`] keeps time itself: each core rotates its
/// sliced priority and wakes due sleepers after its thread ran this many, SVCs included.
// GUESS: a stand-in for ADR-0032's 10 ms timer (T-061 step 3), short enough that sleepers
// wake promptly in tests.
const SLICE: u64 = 10_000;

/// Why the process stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// A thread called `ExitProcess`.
    ExitProcess,
    /// The last started thread called `ExitThread`.
    AllThreadsExited,
    /// The host called [`Ticker::stop`].
    Stopped,
}

struct Shared<'k> {
    kernel: Mutex<Locked<'k>>,
    changed: Condvar,
    /// Per core: "reschedule requested" (ADR-0032 §2). Set and cleared under the lock.
    interrupt: [AtomicBool; CORE_COUNT],
    /// Instructions per quantum, or `None` when a [`Ticker`] keeps time.
    slice: Option<u64>,
}

struct Locked<'k> {
    kernel: &'k mut Kernel,
    result: Option<Result<Finish, RunError>>,
}

impl<'k> Shared<'k> {
    fn lock(&self) -> MutexGuard<'_, Locked<'k>> {
        self.kernel.lock()
    }

    /// Records the first result and takes every core back.
    fn finish(&self, locked: &mut Locked<'_>, result: Result<Finish, RunError>) {
        locked.result.get_or_insert(result);
        for flag in &self.interrupt {
            flag.store(true, Ordering::Release);
        }
        self.changed.notify_all();
    }

    /// The reselection pass: interrupts every core that runs the wrong thread and wakes the
    /// parked ones, if anything changed since the last pass.
    fn reselect(&self, locked: &mut Locked<'_>) {
        let threads = &mut locked.kernel.threads;
        if !threads.take_changed() {
            return;
        }
        for (core, flag) in self.interrupt.iter().enumerate() {
            if threads.needs_switch(core) {
                flag.store(true, Ordering::Release);
            }
        }
        self.changed.notify_all();
    }
}

/// Keeps time for [`run_cores_with`]: each [`tick`](Self::tick) ends one quantum.
pub struct Ticker<'s, 'k> {
    shared: &'s Shared<'k>,
}

impl Ticker<'_, '_> {
    /// Ends a quantum: rotates every core's sliced priority, wakes the sleepers whose time has
    /// come and reselects.
    pub fn tick(&self) {
        let mut locked = self.shared.lock();
        if locked.result.is_some() {
            return;
        }
        let threads = &mut locked.kernel.threads;
        threads.wake_due(Instant::now());
        for core in 0..CORE_COUNT {
            threads.rotate(core);
        }
        self.shared.reselect(&mut locked);
    }

    /// Stops the process: every core leaves its thread at its next safe point and
    /// [`run_cores_with`] returns [`Finish::Stopped`], unless it already finished.
    pub fn stop(&self) {
        let mut locked = self.shared.lock();
        self.shared.finish(&mut locked, Ok(Finish::Stopped));
    }

    /// Whether the process has finished.
    pub fn is_finished(&self) -> bool {
        self.shared.lock().result.is_some()
    }
}

/// Runs the started threads of `kernel` on six host core threads until the process exits,
/// the last thread exits, or a core stops on an error. Each core keeps time by counting
/// instructions. Every core thread has been joined when it returns.
///
/// # Errors
/// The first error a core hit: a guest fault, a `Break`, an SVC the kernel rejected, an
/// interpreter failure, or a core thread that could not be created.
pub fn run_cores(kernel: &mut Kernel, mem: &AddressSpace) -> Result<Finish, RunError> {
    run_on_cores(kernel, mem, Some(SLICE), |_| {})
}

/// Like [`run_cores`], but quanta end only when `driver` calls [`Ticker::tick`]. `driver` runs
/// on the calling thread while the cores run. When it returns or panics, the process stops
/// ([`Finish::Stopped`]) unless it already finished, so this always returns.
///
/// # Errors
/// As [`run_cores`].
pub fn run_cores_with(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    driver: impl FnOnce(&Ticker<'_, '_>),
) -> Result<Finish, RunError> {
    run_on_cores(kernel, mem, None, |ticker| {
        let _stop = StopOnDrop(ticker.shared);
        driver(ticker);
    })
}

fn run_on_cores(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    slice: Option<u64>,
    driver: impl FnOnce(&Ticker<'_, '_>),
) -> Result<Finish, RunError> {
    if kernel.threads.all_exited() {
        return Err(RunError::NoThreads);
    }
    let shared = Shared {
        kernel: Mutex::new(Locked {
            kernel,
            result: None,
        }),
        changed: Condvar::new(),
        interrupt: std::array::from_fn(|_| AtomicBool::new(false)),
        slice,
    };
    thread::scope(|scope| {
        for core in 0..CORE_COUNT {
            let shared = &shared;
            let spawned = thread::Builder::new()
                .name(format!("lince-core-{core}"))
                .spawn_scoped(scope, move || core_main(shared, mem, core));
            if let Err(source) = spawned {
                let mut locked = shared.lock();
                shared.finish(&mut locked, Err(RunError::Spawn { core, source }));
            }
        }
        driver(&Ticker { shared: &shared });
    });
    let locked = shared.kernel.into_inner();
    locked.kernel.current = None;
    // Every core returns only once a result is set.
    locked.result.unwrap_or(Err(RunError::NoThreads))
}

fn core_main(shared: &Shared<'_>, mem: &AddressSpace, core: usize) {
    let _panic = PanicGuard { shared, core };
    let mut last = None;
    while let Some((index, mut state)) = next_thread(shared, core) {
        if last != Some(index) {
            // The monitor is per core: a reservation never survives a thread switch.
            state.exclusive = None;
            last = Some(index);
        }
        let on = OnCore {
            shared,
            mem,
            core,
            index,
        };
        run_thread(&on, state);
    }
}

/// Stops the process once the driver of [`run_cores_with`] is done, even by panicking, so the
/// cores finish and the scope can join them.
struct StopOnDrop<'s, 'k>(&'s Shared<'k>);

impl Drop for StopOnDrop<'_, '_> {
    fn drop(&mut self) {
        let mut locked = self.0.lock();
        self.0.finish(&mut locked, Ok(Finish::Stopped));
    }
}

/// Stops the process when a core thread panics, so the other cores wake and the scope can
/// join them and pass the panic on, instead of waiting forever.
struct PanicGuard<'s, 'k> {
    shared: &'s Shared<'k>,
    core: usize,
}

impl Drop for PanicGuard<'_, '_> {
    fn drop(&mut self) {
        if thread::panicking() {
            let mut locked = self.shared.lock();
            self.shared
                .finish(&mut locked, Err(RunError::Panicked { core: self.core }));
        }
    }
}

/// Parks until `core` has a ready thread and takes it, or returns `None` once the process
/// has finished.
fn next_thread(shared: &Shared<'_>, core: usize) -> Option<(usize, State)> {
    let mut locked = shared.lock();
    loop {
        if locked.result.is_some() {
            return None;
        }
        locked.kernel.threads.wake_due(Instant::now());
        shared.reselect(&mut locked);
        // Nothing runs here, so a pending request is already answered by this pick.
        shared.interrupt[core].store(false, Ordering::Release);
        if let Some(picked) = locked.kernel.threads.take(core) {
            return Some(picked);
        }
        match locked.kernel.threads.next_wake(core) {
            Some(until) => {
                shared.changed.wait_until(&mut locked, until);
            }
            None => shared.changed.wait(&mut locked),
        }
    }
}

/// A thread on its core: what running it needs.
struct OnCore<'s, 'k, 'm> {
    shared: &'s Shared<'k>,
    mem: &'m AddressSpace,
    core: usize,
    index: usize,
}

/// Runs thread `index` on `core` until it should leave: an SVC rescheduled it, a better
/// thread became runnable, its quantum rotated it out, or the process finished. Its state
/// goes back into the table under the same lock hold that decided it leaves the core, so no
/// other core can take it while this core still holds the state.
fn run_thread(on: &OnCore<'_, '_, '_>, mut state: State) {
    // The instructions left in this quantum, kept across SVCs, when the cores keep time.
    let mut left = on.shared.slice;
    let (mut locked, finished) = loop {
        match step(on, &mut state, left.unwrap_or(u64::MAX)) {
            Step::SafePoint {
                mut locked,
                executed,
            } => {
                count_quantum(on, &mut locked, &mut left, executed);
                on.shared.reselect(&mut locked);
                // Under the lock, after the pass: a request made before this point is answered
                // by the check below, and a later one sets the flag again.
                on.shared.interrupt[on.core].store(false, Ordering::Release);
                if locked.result.is_some() || locked.kernel.threads.needs_switch(on.core) {
                    break (locked, None);
                }
            }
            Step::Leave { locked, finished } => break (locked, finished),
        }
    };
    locked.kernel.threads.put_back(on.index, state);
    let all_exited = locked.kernel.threads.all_exited();
    let finished = finished.or_else(|| all_exited.then_some(Ok(Finish::AllThreadsExited)));
    if let Some(result) = finished {
        on.shared.finish(&mut locked, result);
    } else {
        on.shared.reselect(&mut locked);
    }
}

/// Counts `executed` against the quantum when the cores keep time; at its end, wakes the due
/// sleepers and rotates the core's sliced priority.
fn count_quantum(
    on: &OnCore<'_, '_, '_>,
    locked: &mut Locked<'_>,
    left: &mut Option<u64>,
    executed: u64,
) {
    let (Some(left), Some(slice)) = (left, on.shared.slice) else {
        return;
    };
    *left = left.saturating_sub(executed);
    if *left == 0 {
        let threads = &mut locked.kernel.threads;
        threads.wake_due(Instant::now());
        threads.rotate(on.core);
        *left = slice;
    }
}

/// What one interpreter call and its SVC, if any, decided. Both variants hold the lock.
enum Step<'g, 'k> {
    /// A safe point where the thread may keep its core: an SVC that resumes, the instruction
    /// limit, or an interrupt; `executed` instructions ran since the last one.
    SafePoint {
        locked: MutexGuard<'g, Locked<'k>>,
        executed: u64,
    },
    /// The thread leaves its core; `finished` says how the process ended, if it did.
    Leave {
        locked: MutexGuard<'g, Locked<'k>>,
        finished: Option<Result<Finish, RunError>>,
    },
}

fn step<'s, 'k>(on: &OnCore<'s, 'k, '_>, state: &mut State, limit: u64) -> Step<'s, 'k> {
    let leave = |finished| Step::Leave {
        locked: on.shared.lock(),
        finished,
    };
    let result = match run_interruptible(state, on.mem, limit, &on.shared.interrupt[on.core]) {
        Ok(result) => result,
        Err(error) => return leave(Some(Err(RunError::Cpu(error)))),
    };
    let number = match result.exit {
        Exit::Svc { number } => number,
        Exit::Interrupted | Exit::Limit => {
            return Step::SafePoint {
                locked: on.shared.lock(),
                executed: result.executed,
            };
        }
        exit @ (Exit::Undefined { .. }
        | Exit::Unimplemented { .. }
        | Exit::FetchAbort { .. }
        | Exit::PcMisaligned { .. }
        | Exit::DataAbort { .. }) => {
            return leave(Some(Err(RunError::Fault {
                core: on.core,
                exit,
            })));
        }
    };
    let Ok(number) = u8::try_from(number) else {
        return leave(Some(Err(RunError::Kernel(KernelError::UnknownSvc {
            number: u32::from(number),
        }))));
    };
    let mut regs = Regs::default();
    let args = regs.len();
    regs.copy_from_slice(&state.x[..args]);
    let mut locked = on.shared.lock();
    locked.kernel.current = Some(Current {
        index: on.index,
        core: on.core,
    });
    let outcome = locked.kernel.run_svc(on.mem, number, &mut regs);
    locked.kernel.current = None;
    state.x[..args].copy_from_slice(&regs);
    let finished = match outcome {
        Ok(Outcome::Resume) => {
            return Step::SafePoint {
                locked,
                executed: result.executed,
            };
        }
        Ok(Outcome::Reschedule) => None,
        Ok(Outcome::Exit) => Some(Ok(Finish::ExitProcess)),
        Ok(Outcome::Break(reason)) => Some(Err(RunError::Break { reason })),
        Ok(Outcome::Unsupported) => {
            unreachable!("invariant: run_svc turns Unsupported into an error")
        }
        Err(error) => Some(Err(RunError::Kernel(error))),
    };
    Step::Leave { locked, finished }
}
