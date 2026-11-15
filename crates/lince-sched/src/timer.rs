// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kernel timer role thread (ADR-0032 §3): ends a quantum at a fixed period and fires early
//! for wakeups the caller asks for.

use std::thread::{self, Scope, ScopedJoinHandle};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex, MutexGuard};

use crate::{Result, SchedError};

/// Name of the host thread, as shown by debuggers and profilers.
const THREAD_NAME: &str = "lince-kernel-timer";

/// Why the timer fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fire {
    /// A quantum ended.
    Quantum,
    /// A wakeup asked for with [`Timer::wake_at`] is due.
    Wake,
}

/// A periodic timer whose thread runs inside a caller's [`thread::scope`], so the callback may
/// borrow from it. [`Timer::wake_at`] and [`Timer::stop`] run on any thread, guest cores
/// included: they hold a short internal lock, never wait and perform no I/O.
#[derive(Debug, Default)]
pub struct Timer {
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Debug, Default)]
struct State {
    wake: Option<Instant>,
    stopped: bool,
}

impl Timer {
    /// A timer with no wakeup pending; it runs once [`spawn`](Self::spawn)ed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the timer to fire no later than `at`. A later `at` than one already pending changes
    /// nothing.
    pub fn wake_at(&self, at: Instant) {
        let mut state = self.lock();
        if state.wake.is_none_or(|wake| at < wake) {
            state.wake = Some(at);
            self.changed.notify_all();
        }
    }

    /// Ends the timer thread before its next fire; it never fires again.
    pub fn stop(&self) {
        self.lock().stopped = true;
        self.changed.notify_all();
    }

    /// Starts the timer thread in `scope`. Every `quantum` it calls `on_fire(Fire::Quantum)`,
    /// and between quanta it calls `on_fire(Fire::Wake)` when a wakeup is due. `on_fire` runs on
    /// the timer thread without the timer's lock held. The thread ends after [`stop`](Self::stop).
    ///
    /// # Errors
    /// [`SchedError::Spawn`] if the host refuses to create the thread.
    pub fn spawn<'scope, 'env>(
        &'scope self,
        scope: &'scope Scope<'scope, 'env>,
        quantum: Duration,
        mut on_fire: impl FnMut(Fire) + Send + 'scope,
    ) -> Result<ScopedJoinHandle<'scope, ()>> {
        thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn_scoped(scope, move || {
                let mut next_quantum = Instant::now() + quantum;
                while let Some(fire) = self.next_fire(&mut next_quantum, quantum) {
                    on_fire(fire);
                }
            })
            .map_err(|source| SchedError::Spawn {
                name: THREAD_NAME,
                source,
            })
    }

    /// Waits for the next quantum end or due wakeup; `None` once stopped.
    fn next_fire(&self, next_quantum: &mut Instant, quantum: Duration) -> Option<Fire> {
        let mut state = self.lock();
        loop {
            if state.stopped {
                return None;
            }
            let now = Instant::now();
            if now >= *next_quantum {
                // A timer more than a quantum late starts afresh instead of catching up.
                *next_quantum = (*next_quantum + quantum).max(now + quantum);
                state.wake = state.wake.filter(|&wake| wake > now);
                return Some(Fire::Quantum);
            }
            if state.wake.is_some_and(|wake| wake <= now) {
                state.wake = None;
                return Some(Fire::Wake);
            }
            let deadline = state
                .wake
                .map_or(*next_quantum, |wake| wake.min(*next_quantum));
            self.changed.wait_until(&mut state, deadline);
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock()
    }
}
