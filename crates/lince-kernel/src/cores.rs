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
//!
//! Time comes from the kernel timer role thread of `lince-sched` (ADR-0032 §3): every quantum
//! it rotates the sliced priorities, and between quanta it fires when a sleeper is due, so a
//! sleeper wakes on time even while its core runs another thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use lince_cpu_a64::{Exit, State, run_interruptible};
use lince_mem::AddressSpace;
use lince_sched::{Fire, Timer};
use parking_lot::{Condvar, Mutex, MutexGuard};

use crate::error::{KernelError, RunError};
use crate::kernel::{Current, Kernel, Outcome, Regs};
use crate::thread::CORE_COUNT;

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
    /// Runs only for [`run_cores`]; under [`run_cores_with`] wakeup requests go unheard and
    /// the driver keeps time.
    timer: Timer,
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
        self.timer.stop();
        self.changed.notify_all();
    }

    /// One fire of the timer, or one [`Ticker::tick`]: wakes the due sleepers, rotates the
    /// sliced priorities at a quantum end, and reselects.
    fn fire(&self, fire: Fire) {
        let mut locked = self.lock();
        if locked.result.is_some() {
            return;
        }
        let threads = &mut locked.kernel.threads;
        threads.wake_due(Instant::now());
        if fire == Fire::Quantum {
            for core in 0..CORE_COUNT {
                threads.rotate(core);
            }
        }
        // A fire consumes the timer's pending wakeup; re-arm it for the next sleeper even when
        // this fire changed nothing.
        if let Some(at) = threads.next_wake_any() {
            self.timer.wake_at(at);
        }
        self.reselect(&mut locked);
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
        if let Some(at) = threads.next_wake_any() {
            self.timer.wake_at(at);
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
        self.shared.fire(Fire::Quantum);
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
/// the last thread exits, or a core stops on an error. The kernel timer role thread keeps
/// time. Every core thread and the timer thread have been joined when it returns.
///
/// # Errors
/// The first error a core hit: a guest fault, a `Break`, an SVC the kernel rejected, an
/// interpreter failure, or a core or timer thread that could not be created.
pub fn run_cores(kernel: &mut Kernel, mem: &AddressSpace) -> Result<Finish, RunError> {
    let quantum = kernel.quantum;
    run_on_cores(kernel, mem, Some(quantum), |_| {})
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
    quantum: Option<Duration>,
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
        timer: Timer::new(),
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
        if let Some(quantum) = quantum {
            let shared = &shared;
            let spawned = shared.timer.spawn(scope, quantum, move |fire| {
                let _panic = TimerPanicGuard(shared);
                shared.fire(fire);
            });
            if let Err(source) = spawned {
                let mut locked = shared.lock();
                shared.finish(&mut locked, Err(RunError::Timer(source)));
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

/// Stops the process when the timer callback panics, so the cores stop instead of running
/// on with no one keeping time.
struct TimerPanicGuard<'s, 'k>(&'s Shared<'k>);

impl Drop for TimerPanicGuard<'_, '_> {
    fn drop(&mut self) {
        if thread::panicking() {
            let mut locked = self.0.lock();
            self.0.finish(&mut locked, Err(RunError::TimerPanicked));
        }
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
    let (mut locked, finished) = loop {
        match step(on, &mut state) {
            Step::SafePoint(mut locked) => {
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

/// What one interpreter call and its SVC, if any, decided. Both variants hold the lock.
enum Step<'g, 'k> {
    /// A safe point where the thread may keep its core: an SVC that resumes, or an interrupt.
    SafePoint(MutexGuard<'g, Locked<'k>>),
    /// The thread leaves its core; `finished` says how the process ended, if it did.
    Leave {
        locked: MutexGuard<'g, Locked<'k>>,
        finished: Option<Result<Finish, RunError>>,
    },
}

fn step<'s, 'k>(on: &OnCore<'s, 'k, '_>, state: &mut State) -> Step<'s, 'k> {
    let leave = |finished| Step::Leave {
        locked: on.shared.lock(),
        finished,
    };
    // No instruction limit: the core comes back only at an SVC or when interrupted.
    let interrupt = &on.shared.interrupt[on.core];
    let result = match run_interruptible(state, on.mem, u64::MAX, interrupt) {
        Ok(result) => result,
        Err(error) => return leave(Some(Err(RunError::Cpu(error)))),
    };
    let number = match result.exit {
        Exit::Svc { number } => number,
        Exit::Interrupted | Exit::Limit => return Step::SafePoint(on.shared.lock()),
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
        Ok(Outcome::Resume) => return Step::SafePoint(locked),
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
