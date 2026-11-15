// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Once-per-name recording and draining through the public API (QUAL-73).

use std::thread;

use lince_trace::{EventKind, ThreadRole, Trace};

#[test]
fn a_stub_is_recorded_only_the_first_time_it_runs() {
    let mut trace = Trace::new(16);
    let name = trace.name("svcBreak").unwrap();
    let mut recorder = trace.recorder(ThreadRole::GuestCore);

    let first = recorder.stub(name);
    let again = recorder.stub(name);

    assert!(first);
    assert!(!again);
    let drained = trace.drain();
    assert_eq!(drained.records.len(), 1);
    assert_eq!(drained.dropped, 0);
    let record = drained.records[0];
    assert_eq!(record.role, ThreadRole::GuestCore);
    assert_eq!(record.event.kind, EventKind::Stub);
    assert_eq!(trace.stub_name(&record.event), Some("svcBreak"));
}

#[test]
fn different_names_and_kinds_are_each_recorded_once() {
    let mut trace = Trace::new(16);
    let a = trace.name("a").unwrap();
    let b = trace.name("b").unwrap();
    let mut recorder = trace.recorder(ThreadRole::GuestCore);

    for _ in 0..3 {
        recorder.stub(a);
        recorder.stub(b);
        recorder.once(EventKind::UnknownSvc, 0x80);
        recorder.once(EventKind::UnknownSvc, 0x81);
    }

    let drained = trace.drain();
    assert_eq!(drained.records.len(), 4);
    let names: Vec<_> = drained
        .records
        .iter()
        .filter_map(|r| trace.stub_name(&r.event))
        .collect();
    assert_eq!(names, ["a", "b"]);
}

#[test]
fn an_unknown_command_is_recorded_once_per_object_and_command() {
    let mut trace = Trace::new(16);
    let sm = trace.name("sm:").unwrap();
    let mut recorder = trace.recorder(ThreadRole::GuestCore);

    assert!(recorder.unknown_command(sm, 7));
    assert!(!recorder.unknown_command(sm, 7));
    assert!(recorder.unknown_command(sm, 8));

    let drained = trace.drain();
    let commands: Vec<_> = drained
        .records
        .iter()
        .filter_map(|r| trace.unknown_command(&r.event))
        .collect();
    assert_eq!(commands, [("sm:", 7), ("sm:", 8)]);
    assert_eq!(trace.stub_name(&drained.records[0].event), None);
}

#[test]
fn registering_a_name_twice_returns_the_same_id() {
    let mut trace = Trace::new(1);

    let first = trace.name("svcBreak").unwrap();
    let second = trace.name("svcBreak").unwrap();

    assert_eq!(first, second);
    assert_ne!(first, trace.name("other").unwrap());
}

#[test]
fn draining_empties_the_buffers() {
    let mut trace = Trace::new(4);
    let mut recorder = trace.recorder(ThreadRole::Other);
    recorder.once(EventKind::UnknownSvc, 1);

    assert_eq!(trace.drain().records.len(), 1);
    assert_eq!(trace.drain().records, []);

    recorder.once(EventKind::UnknownSvc, 2);
    assert_eq!(trace.drain().records[0].event.arg, 2);
}

#[test]
fn recording_past_capacity_counts_drops_instead_of_blocking() {
    let mut trace = Trace::new(2);
    let mut recorder = trace.recorder(ThreadRole::GuestCore);

    recorder.once(EventKind::UnknownSvc, 1);
    recorder.once(EventKind::UnknownSvc, 2);
    let third = recorder.once(EventKind::UnknownSvc, 3);

    assert!(!third);
    let drained = trace.drain();
    assert_eq!(drained.records.len(), 2);
    assert_eq!(drained.dropped, 1);
    assert_eq!(trace.drain().dropped, 0);
}

#[test]
fn events_from_other_threads_are_drained_with_their_role() {
    let mut trace = Trace::new(8);
    let name = trace.name("hle stub").unwrap();
    let mut core = trace.recorder(ThreadRole::GuestCore);
    let mut worker = trace.recorder(ThreadRole::CompileWorker);

    thread::scope(|s| {
        s.spawn(move || core.stub(name));
        s.spawn(move || worker.once(EventKind::UnknownSvc, 0x80));
    });

    let drained = trace.drain();
    let mut roles: Vec<_> = drained.records.iter().map(|r| r.role).collect();
    roles.sort_by_key(|role| role.to_raw());
    assert_eq!(roles, [ThreadRole::GuestCore, ThreadRole::CompileWorker]);
    assert_eq!(trace.drain().records, []);
}

#[test]
fn a_full_duplicate_set_counts_new_names_as_dropped() {
    let mut trace = Trace::new(2);
    let mut recorder = trace.recorder(ThreadRole::GuestCore);
    recorder.once(EventKind::UnknownSvc, 1);
    recorder.once(EventKind::UnknownSvc, 2);
    trace.drain();

    let third = recorder.once(EventKind::UnknownSvc, 3);

    assert!(!third);
    let drained = trace.drain();
    assert_eq!(drained.records, []);
    assert_eq!(drained.dropped, 1);
}

#[test]
fn plain_records_keep_every_event_in_order() {
    let mut trace = Trace::new(8);
    let mut recorder = trace.recorder(ThreadRole::GuestCore);
    for ns in [30, 10, 30] {
        assert!(recorder.record(EventKind::KernelLockHeld, ns));
    }
    assert!(recorder.record(EventKind::PreemptLatency, 5));

    let drained = trace.drain();

    let events: Vec<_> = drained
        .records
        .iter()
        .map(|record| (record.role, record.event.kind, record.event.arg))
        .collect();
    assert_eq!(
        events,
        [
            (ThreadRole::GuestCore, EventKind::KernelLockHeld, 30),
            (ThreadRole::GuestCore, EventKind::KernelLockHeld, 10),
            (ThreadRole::GuestCore, EventKind::KernelLockHeld, 30),
            (ThreadRole::GuestCore, EventKind::PreemptLatency, 5),
        ]
    );
    assert_eq!(drained.dropped, 0);
}

#[test]
fn plain_records_past_capacity_are_counted_as_dropped() {
    let mut trace = Trace::new(2);
    let mut recorder = trace.recorder(ThreadRole::GuestCore);
    let accepted: Vec<_> = (0..3)
        .map(|ns| recorder.record(EventKind::KernelLockHeld, ns))
        .collect();

    assert_eq!(accepted, [true, true, false]);
    let drained = trace.drain();
    assert_eq!((drained.records.len(), drained.dropped), (2, 1));
}
