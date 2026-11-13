// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ledger behavior: registration limits, concurrent dumps, dump format.
#![cfg(test)]

use std::sync::atomic::{AtomicBool, Ordering};

use poc_frame_ledger::{Dump, DumpError, Event, EventKind, Ledger, ThreadRole};

/// Argument derived from the timestamp, so a torn event is detectable.
fn arg_for(ts: u64) -> u64 {
    ts.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA5A5
}

#[test]
fn registration_stops_at_the_thread_limit() {
    let ledger = Ledger::new(2, 4);
    assert!(ledger.register(ThreadRole::GpuBack).is_some());
    assert!(ledger.register(ThreadRole::GpuFront).is_some());
    assert!(ledger.register(ThreadRole::Other).is_none());
    assert_eq!(ledger.dump().threads.len(), 2);
}

/// Every event in the dump is internally consistent and in order.
fn assert_not_torn(dump: &Dump) {
    for thread in &dump.threads {
        let mut last = 0;
        for e in &thread.events {
            assert_eq!(e.arg, arg_for(e.ts_ns), "torn event {e:?}");
            assert_eq!(u64::from(e.frame), e.ts_ns >> 8, "torn event {e:?}");
            assert!(e.ts_ns > last, "events out of order");
            last = e.ts_ns;
        }
    }
}

/// Records 200,000 events; halfway it waits until the reader confirms a mid-recording dump.
fn produce(
    ledger: &Ledger,
    half_written: &AtomicBool,
    checked_half: &AtomicBool,
    done: &AtomicBool,
) {
    let recorder = ledger.register(ThreadRole::GpuBack).unwrap();
    for ts in 1..=200_000u64 {
        if ts == 100_001 {
            half_written.store(true, Ordering::Release);
            while !checked_half.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        }
        let frame = u32::try_from(ts >> 8).unwrap();
        recorder.record_at(ts, EventKind::QueueSubmit, frame, arg_for(ts));
    }
    done.store(true, Ordering::Release);
}

#[test]
fn dumps_taken_during_recording_never_contain_torn_events() {
    let ledger = Ledger::new(1, 8);
    let half_written = AtomicBool::new(false);
    let checked_half = AtomicBool::new(false);
    let done = AtomicBool::new(false);
    let mut partial_dumps = 0;
    std::thread::scope(|s| {
        s.spawn(|| produce(&ledger, &half_written, &checked_half, &done));
        while !done.load(Ordering::Acquire) {
            let dump = ledger.dump();
            assert_not_torn(&dump);
            let newest = dump
                .threads
                .first()
                .and_then(|t| t.events.last())
                .map(|e| e.ts_ns);
            if newest.is_some_and(|ts| ts < 200_000) {
                partial_dumps += 1;
            }
            if half_written.load(Ordering::Acquire) {
                checked_half.store(true, Ordering::Release);
            }
        }
    });
    assert!(partial_dumps >= 1, "no dump overlapped the recording");
    let final_dump = ledger.dump();
    assert_not_torn(&final_dump);
    let thread = &final_dump.threads[0];
    assert_eq!(thread.events.len(), 256);
    assert_eq!(thread.lost_events, 200_000 - 256);
    assert_eq!(thread.events.last().unwrap().ts_ns, 200_000);
}

#[test]
fn record_uses_a_monotonic_clock() {
    let ledger = Ledger::new(1, 4);
    let recorder = ledger.register(ThreadRole::Present).unwrap();
    recorder.record(EventKind::FrameBegin, 1, 0);
    recorder.record(EventKind::FrameEnd, 1, 0);
    let events = ledger.dump().threads.remove(0).events;
    assert!(events[0].ts_ns <= events[1].ts_ns);
}

#[test]
fn dump_round_trips_through_the_binary_format() {
    let ledger = Ledger::new(2, 4);
    let a = ledger.register(ThreadRole::GpuBack).unwrap();
    let b = ledger.register(ThreadRole::CompileWorker).unwrap();
    a.record_at(10, EventKind::PipelineWait, 3, 5_000);
    b.record_at(11, EventKind::CompileEnd, 3, 9_000);
    b.record_at(12, EventKind::Unknown(4000), 4, 1);
    let dump = ledger.dump();
    let mut bytes = Vec::new();
    dump.write_to(&mut bytes).unwrap();
    assert_eq!(Dump::read_from(&mut bytes.as_slice()).unwrap(), dump);
    assert_eq!(
        dump.threads[1].events[1],
        Event {
            ts_ns: 12,
            frame: 4,
            kind: EventKind::Unknown(4000),
            arg: 1
        }
    );
}

#[test]
fn reading_rejects_foreign_and_truncated_data() {
    assert!(matches!(
        Dump::read_from(&mut &b"NOPE\x01\0\0\0\0\0\0\0"[..]),
        Err(DumpError::BadMagic)
    ));
    assert!(matches!(
        Dump::read_from(&mut &b"LNXL\x02\0\0\0\0\0\0\0"[..]),
        Err(DumpError::UnsupportedVersion(2))
    ));
    let ledger = Ledger::new(1, 4);
    ledger
        .register(ThreadRole::GpuBack)
        .unwrap()
        .record_at(1, EventKind::FenceWait, 1, 1);
    let mut bytes = Vec::new();
    ledger.dump().write_to(&mut bytes).unwrap();
    bytes.truncate(bytes.len() - 1);
    assert!(matches!(
        Dump::read_from(&mut bytes.as_slice()),
        Err(DumpError::Io(_))
    ));
}
