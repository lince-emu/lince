// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The six guest core threads (ADR-0017 B2 scope): created when the process runs, parked
//! while their core has no ready thread, joined before [`run_cores`] returns.
//!
//! The kernel sits behind one mutex. A core holds it only to pick a thread and to dispatch
//! an SVC; guest code runs with the lock released. After any SVC every parked core wakes to
//! look again, since the SVC may have started, moved or woken a thread.

use std::thread;
use std::time::Instant;

use lince_cpu_a64::{Exit, State, run};
use lince_mem::AddressSpace;
use parking_lot::{Condvar, Mutex, MutexGuard};

use crate::error::{KernelError, RunError};
use crate::kernel::{Current, Kernel, Outcome, Regs};
use crate::thread::CORE_COUNT;

/// Instructions a thread runs before its core picks again; equal-priority threads take turns
/// at this granularity.
// GUESS: short enough for fair turns in tests, long enough that locking is rare.
const SLICE: u64 = 10_000;

/// Why the process stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// A thread called `ExitProcess`.
    ExitProcess,
    /// The last started thread called `ExitThread`.
    AllThreadsExited,
}

struct Shared<'k> {
    kernel: Mutex<Locked<'k>>,
    changed: Condvar,
}

struct Locked<'k> {
    kernel: &'k mut Kernel,
    result: Option<Result<Finish, RunError>>,
}

impl<'k> Shared<'k> {
    fn lock(&self) -> MutexGuard<'_, Locked<'k>> {
        self.kernel.lock()
    }

    fn finish(&self, locked: &mut Locked<'_>, result: Result<Finish, RunError>) {
        locked.result.get_or_insert(result);
        self.changed.notify_all();
    }
}

/// Runs the started threads of `kernel` on six host core threads until the process exits,
/// the last thread exits, or a core stops on an error. Every core thread has been joined when
/// it returns.
///
/// # Errors
/// The first error a core hit: a guest fault, a `Break`, an SVC the kernel rejected, an
/// interpreter failure, or a core thread that could not be created.
pub fn run_cores(kernel: &mut Kernel, mem: &AddressSpace) -> Result<Finish, RunError> {
    if kernel.threads.all_exited() {
        return Err(RunError::NoThreads);
    }
    let shared = Shared {
        kernel: Mutex::new(Locked {
            kernel,
            result: None,
        }),
        changed: Condvar::new(),
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
        let now = Instant::now();
        if let Some(picked) = locked.kernel.threads.pick(core, now) {
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

/// Runs thread `index` on `core` until its slice ends, an SVC reschedules it or the process
/// finishes. Its state goes back into the table under the same lock hold that decided it
/// leaves the core, so no other core can pick it while this core still holds the state.
fn run_thread(on: &OnCore<'_, '_, '_>, mut state: State) {
    let mut left = SLICE;
    let (mut locked, finished) = loop {
        match step(on, &mut state, left) {
            Step::Resume { locked, executed } => {
                left = left.saturating_sub(executed);
                // A thread that keeps making SVCs still gives up its core when its slice ends.
                if left == 0 {
                    break (locked, None);
                }
                notify_if_changed(on.shared, locked);
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
        locked.kernel.threads.take_changed();
        on.shared.changed.notify_all();
    }
}

/// Wakes the parked cores if the last SVC readied, moved or reprioritized a thread.
fn notify_if_changed(shared: &Shared<'_>, mut locked: MutexGuard<'_, Locked<'_>>) {
    if locked.kernel.threads.take_changed() {
        shared.changed.notify_all();
    }
}

/// What one interpreter call and its SVC, if any, decided. Both variants hold the lock.
enum Step<'g, 'k> {
    /// The SVC ran; the thread may keep its core.
    Resume {
        locked: MutexGuard<'g, Locked<'k>>,
        executed: u64,
    },
    /// The thread leaves its core; `finished` says how the process ended, if it did.
    Leave {
        locked: MutexGuard<'g, Locked<'k>>,
        finished: Option<Result<Finish, RunError>>,
    },
}

fn step<'s, 'k>(on: &OnCore<'s, 'k, '_>, state: &mut State, left: u64) -> Step<'s, 'k> {
    let leave = |finished| Step::Leave {
        locked: on.shared.lock(),
        finished,
    };
    let result = match run(state, on.mem, left) {
        Ok(result) => result,
        Err(error) => return leave(Some(Err(RunError::Cpu(error)))),
    };
    let number = match result.exit {
        Exit::Svc { number } => number,
        Exit::Limit | Exit::Interrupted => return leave(None),
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
            return Step::Resume {
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
