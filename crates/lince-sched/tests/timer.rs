// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The kernel timer role thread: periodic quanta, early wakeups and stop.

use std::thread;
use std::time::{Duration, Instant};

use lince_sched::{Fire, Timer};
use parking_lot::Mutex;

/// Long enough that a wakeup in its middle is clearly not the quantum end.
const LONG: Duration = Duration::from_secs(10);

#[test]
fn quanta_fire_at_the_period_until_stopped() {
    const QUANTUM: Duration = Duration::from_millis(5);
    let timer = Timer::new();
    let fires = Mutex::new(Vec::new());
    let start = Instant::now();
    thread::scope(|scope| {
        timer
            .spawn(scope, QUANTUM, |fire| {
                let mut fires = fires.lock();
                fires.push((fire, start.elapsed()));
                if fires.len() == 3 {
                    timer.stop();
                }
            })
            .expect("spawn");
    });
    let fires = fires.into_inner();
    assert_eq!(fires.len(), 3, "no fire after stop");
    for (n, (fire, at)) in (1u32..).zip(&fires) {
        assert_eq!(*fire, Fire::Quantum);
        assert!(*at >= QUANTUM * n, "quantum {n} came early: {at:?}");
    }
}

#[test]
fn a_wakeup_fires_before_the_quantum_ends() {
    const DELAY: Duration = Duration::from_millis(20);
    let timer = Timer::new();
    let mut fired = None;
    let start = Instant::now();
    thread::scope(|scope| {
        timer
            .spawn(scope, LONG, |fire| {
                fired = Some((fire, start.elapsed()));
                timer.stop();
            })
            .expect("spawn");
        // A later request does not push back an earlier one.
        timer.wake_at(start + DELAY);
        timer.wake_at(start + LONG / 2);
    });
    let (fire, at) = fired.expect("the timer fired");
    assert_eq!(fire, Fire::Wake);
    assert!(at >= DELAY && at < LONG / 2, "woke at {at:?}");
}

#[test]
fn stop_ends_an_idle_timer_promptly() {
    let timer = Timer::new();
    let start = Instant::now();
    thread::scope(|scope| {
        timer
            .spawn(scope, LONG, |_| panic!("the timer must not fire"))
            .expect("spawn");
        timer.stop();
    });
    assert!(start.elapsed() < LONG);
}
