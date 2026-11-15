// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loom model of the thread-to-core handoff (ADR-0032 §4 and its validation plan): while a
//! thread runs on core 0, another host thread moves it to core 1 (`SetThreadCoreMask` on a
//! running thread). Under every interleaving loom explores:
//! - the thread's state is held by at most one core, and core 1 takes it only after core 0
//!   gave it back, with its registers intact;
//! - the handoff always completes: core 0 sees the reschedule request (the interrupt flag is
//!   never lost) and parked core 1 is woken (the notify is never lost).
//!
//! It drives the real thread table (`take`, `put_back`, `switch_requests`, `needs_switch`) with
//! the protocol of `cores.rs`: one lock; a reselection pass after every change that sets the
//! interrupt flag of each core that must switch and notifies parked cores; core 0 running
//! "guest code" that reaches a safe point once its flag is set (as `run_interruptible` does at a
//! block boundary), clearing the flag under the lock; leaving and picking in separate lock
//! holds (`run_thread`, then `next_thread`); and core 1 parked on the condition variable until
//! it can take a thread. Run it with `cargo xtask ci`, or by hand:
//! `RUSTFLAGS="--cfg loom" cargo test -p lince-kernel --lib --release loom`.

use loom::sync::atomic::{AtomicBool, Ordering};
use loom::sync::{Arc, Condvar, Mutex, MutexGuard};
use loom::thread;

use lince_common::GuestVa;
use lince_cpu_a64::State;

use crate::thread::{Status, ThreadParams, Threads};

/// The `pc` the moved thread carries, to show its state survives the move.
const MARK: u64 = 0x1234;

struct Model {
    threads: Threads,
    /// Which core holds the state of thread 0, as the model's cores report it.
    holder: Option<usize>,
}

struct Shared {
    model: Mutex<Model>,
    changed: Condvar,
    interrupt: [AtomicBool; 2],
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Model> {
        self.model.lock().expect("loom lock")
    }

    /// The reselection pass of `cores.rs`.
    fn reselect(&self, model: &mut Model) {
        if let Some(switch) = model.threads.switch_requests() {
            let requested = self
                .interrupt
                .iter()
                .zip(switch)
                .filter(|(_, switch)| *switch);
            for (flag, _) in requested {
                flag.store(true, Ordering::Release);
            }
            self.changed.notify_all();
        }
    }
}

/// Core 0 runs the thread until its flag asks for a safe point at which it must switch, then
/// gives the state back (`run_thread`) and looks for other work in a new lock hold
/// (`next_thread`), finding none.
fn core0(shared: &Shared, state: State) {
    loop {
        // Guest code: `run_interruptible` returns at the next block boundary once the flag is
        // set. If the request were lost, this would spin forever and loom would fail.
        while !shared.interrupt[0].load(Ordering::Acquire) {
            thread::yield_now();
        }
        let mut model = shared.lock();
        shared.reselect(&mut model);
        shared.interrupt[0].store(false, Ordering::Release);
        if model.threads.needs_switch(0) {
            model.threads.put_back(0, state);
            assert_eq!(model.holder.take(), Some(0), "core 0 held the state");
            shared.reselect(&mut model);
            break;
        }
    }
    let mut model = shared.lock();
    shared.reselect(&mut model);
    assert!(
        model.threads.take(0).is_none(),
        "nothing else runs on core 0"
    );
}

/// Core 1 parks until it can take a thread (`next_thread`) and returns its state.
fn core1(shared: &Shared) -> State {
    let mut model = shared.lock();
    loop {
        shared.reselect(&mut model);
        shared.interrupt[1].store(false, Ordering::Release);
        if let Some((index, state)) = model.threads.take(1) {
            assert_eq!(index, 0);
            assert_eq!(model.holder, None, "core 1 took a state core 0 still holds");
            model.holder = Some(1);
            return state;
        }
        model = shared.changed.wait(model).expect("loom wait");
    }
}

#[test]
fn moving_a_running_thread_hands_it_over_once_and_always() {
    loom::model(|| {
        let mut threads = Threads::default();
        let params = ThreadParams {
            entry: GuestVa::new(MARK),
            arg: 0,
            stack_top: 0,
            priority: 44,
            core: 0,
        };
        threads.insert(GuestVa::new(0), params);
        threads.make_ready(0);
        let (_, state) = threads.take(0).expect("core 0 runs it");
        let shared = Arc::new(Shared {
            model: Mutex::new(Model {
                threads,
                holder: Some(0),
            }),
            changed: Condvar::new(),
            interrupt: [AtomicBool::new(false), AtomicBool::new(false)],
        });

        let mover = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                let mut model = shared.lock();
                let thread = model.threads.thread_mut(0);
                thread.core = 1;
                thread.mask = 0b10;
                shared.reselect(&mut model);
            })
        };
        let parked = {
            let shared = Arc::clone(&shared);
            thread::spawn(move || core1(&shared))
        };
        core0(&shared, state);
        let handed = parked.join().expect("core 1");
        mover.join().expect("mover");

        assert_eq!(handed.pc, MARK, "the registers moved with the thread");
        let model = shared.lock();
        assert_eq!(model.holder, Some(1));
        assert_eq!(model.threads.status(0), Status::Running { core: 1 });
    });
}
