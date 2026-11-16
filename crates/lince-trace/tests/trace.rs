// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Once-per-name recording and draining through the public API (QUAL-73).

use std::thread;

use lince_trace::{EventKind, HostIoError, ThreadRole, Trace};

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
    assert_eq!((drained.dropped, drained.dropped_once), (1, 1));
    assert_eq!(trace.drain().dropped_once, 0);
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
    assert_eq!((drained.dropped, drained.dropped_once), (1, 1));
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
    assert_eq!(drained.dropped_once, 0, "only plain records were lost");
}

#[test]
fn the_coverage_list_holds_each_unknown_svc_and_command_once_in_order() {
    let mut trace = Trace::new(16);
    let set = trace.name("set").unwrap();
    let stub = trace.name("svcBreak").unwrap();
    let mut core0 = trace.recorder(ThreadRole::GuestCore);
    let mut core1 = trace.recorder(ThreadRole::GuestCore);

    core0.once(EventKind::UnknownSvc, 0x7F);
    core0.unknown_command(set, 5);
    core0.stub(stub);
    core1.once(EventKind::UnknownSvc, 0x7F);
    core1.once(EventKind::UnknownSvc, 0x2F);

    let drained = trace.drain();
    let list: Vec<String> = trace
        .unimplemented(&drained)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(list, ["SVC 0x2f", "SVC 0x7f", "set command 5"]);
}

#[test]
fn a_run_without_unknown_calls_has_an_empty_coverage_list() {
    let mut trace = Trace::new(16);
    let stub = trace.name("svcBreak").unwrap();
    trace.recorder(ThreadRole::GuestCore).stub(stub);
    let drained = trace.drain();
    assert_eq!(trace.unimplemented(&drained), []);
}

#[test]
fn a_host_io_failure_is_recorded_once_per_object_command_and_error() {
    let mut trace = Trace::new(16);
    let file = trace.name("fsp-srv.IFile").unwrap();
    let mut recorder = trace.recorder(ThreadRole::Other);

    assert!(recorder.host_io_failed(file, 0, HostIoError::NotFound));
    assert!(!recorder.host_io_failed(file, 0, HostIoError::NotFound));
    assert!(recorder.host_io_failed(file, 0, HostIoError::GuestMemory));
    assert!(recorder.host_io_failed(file, 1, HostIoError::NotFound));

    let drained = trace.drain();
    let failures: Vec<_> = drained
        .records
        .iter()
        .filter_map(|r| trace.host_io_failure(&r.event))
        .collect();
    assert_eq!(
        failures,
        [
            ("fsp-srv.IFile", 0, HostIoError::NotFound),
            ("fsp-srv.IFile", 0, HostIoError::GuestMemory),
            ("fsp-srv.IFile", 1, HostIoError::NotFound),
        ]
    );
    assert_eq!(trace.unknown_command(&drained.records[0].event), None);
}

#[test]
fn the_failure_list_is_sorted_once_per_failure_and_named() {
    let mut trace = Trace::new(16);
    let file = trace.name("fsp-srv.IFile").unwrap();
    let fs = trace.name("fsp-srv.IFileSystem").unwrap();
    let mut first = trace.recorder(ThreadRole::Other);
    let mut second = trace.recorder(ThreadRole::Other);
    first.host_io_failed(fs, 7, HostIoError::NotFound);
    second.host_io_failed(fs, 7, HostIoError::NotFound);
    second.host_io_failed(file, 1, HostIoError::StorageFull);
    second.host_io_failed(file, 0, HostIoError::Unknown(40));
    first.stub(file);

    let drained = trace.drain();
    let names: Vec<String> = trace
        .host_io_failures(&drained)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        names,
        [
            "fsp-srv.IFile command 0: error class 40",
            "fsp-srv.IFile command 1: storage full",
            "fsp-srv.IFileSystem command 7: not found",
        ]
    );
}
