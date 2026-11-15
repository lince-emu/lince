// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The service-I/O thread: submission, completion signals, a full queue and teardown.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use lince_sched::{Request, SchedError, ServiceIo, Shutdown, Signal, SubmitError};

const TIMEOUT: Duration = Duration::from_secs(10);

/// A signal that reports each firing on a channel, standing in for a kernel event.
struct ChannelSignal(Sender<()>);

impl Signal for ChannelSignal {
    fn signal(&self) {
        self.0.send(()).expect("test receiver alive");
    }
}

fn signal() -> (Arc<dyn Signal>, Receiver<()>) {
    let (tx, rx) = mpsc::channel();
    (Arc::new(ChannelSignal(tx)), rx)
}

/// A request that records `value` into `log` once `gate` (if any) opens.
fn recording(
    log: &Arc<AtomicU64>,
    value: u64,
    done: &Arc<dyn Signal>,
    gate: Option<Receiver<()>>,
) -> Request {
    let log = Arc::clone(log);
    Request::new("record", Arc::clone(done), move || {
        if let Some(gate) = gate {
            gate.recv().expect("gate sender alive");
        }
        log.store(log.load(Ordering::Relaxed) * 10 + value, Ordering::Relaxed);
    })
}

#[test]
fn submitted_request_runs_and_fires_its_completion_signal() {
    let (space, _space_rx) = signal();
    let io = ServiceIo::start(4, space).unwrap();
    let (done, done_rx) = signal();
    let log = Arc::new(AtomicU64::new(0));

    io.try_submit(recording(&log, 7, &done, None)).unwrap();

    done_rx.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(log.load(Ordering::Relaxed), 7);
    assert_eq!(io.shutdown().unwrap(), Shutdown { completed: 1 });
}

#[test]
fn full_queue_hands_the_request_back_and_signals_space_later() {
    let (space, space_rx) = signal();
    let io = ServiceIo::start(1, space).unwrap();
    let (done, done_rx) = signal();
    let log = Arc::new(AtomicU64::new(0));

    // The first request is taken off the queue and blocks on the gate.
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (gate_tx, gate_rx) = mpsc::channel::<()>();
    let first_log = Arc::clone(&log);
    io.try_submit(Request::new("first", Arc::clone(&done), move || {
        started_tx.send(()).unwrap();
        gate_rx.recv().unwrap();
        first_log.store(1, Ordering::Relaxed);
    }))
    .unwrap();
    started_rx.recv_timeout(TIMEOUT).unwrap();

    io.try_submit(recording(&log, 2, &done, None)).unwrap();
    let third = match io.try_submit(recording(&log, 3, &done, None)) {
        Err(SubmitError::Full(request)) => request,
        other => panic!("expected a full queue, got {other:?}"),
    };
    assert!(
        space_rx.try_recv().is_err(),
        "space signalled before the queue drained"
    );

    gate_tx.send(()).unwrap();
    space_rx.recv_timeout(TIMEOUT).unwrap();
    io.try_submit(third).unwrap();

    assert_eq!(io.shutdown().unwrap(), Shutdown { completed: 3 });
    assert_eq!(done_rx.try_iter().count(), 3);
    assert_eq!(log.load(Ordering::Relaxed), 123);
}

#[test]
fn shutdown_completes_every_queued_request_in_order() {
    let (space, _space_rx) = signal();
    let io = ServiceIo::start(4, space).unwrap();
    let (done, done_rx) = signal();
    let log = Arc::new(AtomicU64::new(0));
    let (gate_tx, gate_rx) = mpsc::channel::<()>();

    io.try_submit(recording(&log, 1, &done, Some(gate_rx)))
        .unwrap();
    for value in 2..=4 {
        io.try_submit(recording(&log, value, &done, None)).unwrap();
    }
    gate_tx.send(()).unwrap();

    assert_eq!(io.shutdown().unwrap(), Shutdown { completed: 4 });
    assert_eq!(done_rx.try_iter().count(), 4);
    assert_eq!(log.load(Ordering::Relaxed), 1234);
}

#[test]
fn shutdown_reports_requests_left_by_a_panicked_thread() {
    let (space, _space_rx) = signal();
    let io = ServiceIo::start(2, space).unwrap();
    let (done, done_rx) = signal();

    io.try_submit(Request::new("panics", Arc::clone(&done), || {
        panic!("service bug")
    }))
    .unwrap();

    match io.shutdown() {
        Err(SchedError::Panicked { pending, .. }) => assert_eq!(pending, 1),
        other => panic!("expected a panic report, got {other:?}"),
    }
    assert!(done_rx.try_recv().is_err());
}

#[test]
fn zero_capacity_is_rejected() {
    let (space, _space_rx) = signal();
    assert!(matches!(
        ServiceIo::start(0, space),
        Err(SchedError::ZeroCapacity)
    ));
}
