// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-7 acceptance: the analyzer finds injected synthetic spikes and names the right cause.
#![cfg(test)]

use poc_frame_ledger::{EventKind, Ledger, Recorder, ThreadRole, analyze};

const MS: u64 = 1_000_000;
const FRAME: u64 = 16_666_667;
const TARGET: u64 = 20 * MS;

/// Threads of a synthetic session.
struct Session {
    present: Recorder,
    gpu_back: Recorder,
    gpu_front: Recorder,
    worker: Recorder,
}

/// Injected spikes: (frame, frame length, events as (thread, kind, blocked ns)).
#[derive(Clone, Copy)]
enum On {
    GpuBack,
    GpuFront,
    Worker,
}

/// (frame, frame length, injected events as (thread, kind, blocked ns)).
type Injected = (u32, u64, Vec<(On, EventKind, u64)>);

fn spikes() -> Vec<Injected> {
    vec![
        // Render thread waits for a pipeline: emulator-induced.
        (
            100,
            40 * MS,
            vec![(On::GpuBack, EventKind::PipelineWait, 22 * MS)],
        ),
        // Cache lookup on a critical thread (should never happen): emulator-induced.
        (
            400,
            30 * MS,
            vec![(On::GpuFront, EventKind::CacheResolve, 12 * MS)],
        ),
        // Slow frame with no emulator events: the game's own behavior.
        (600, 35 * MS, vec![]),
        // A long compile on a worker thread does not delay the frame by itself.
        (
            800,
            35 * MS,
            vec![(On::Worker, EventKind::CompileEnd, 50 * MS)],
        ),
        // Small waits: identified, but below half of the overrun.
        (
            900,
            30 * MS,
            vec![
                (On::GpuBack, EventKind::FenceWait, 2 * MS),
                (On::GpuFront, EventKind::PipelineWait, MS),
            ],
        ),
    ]
}

/// Records 1000 frames of ~16.7 ms (±0.5 ms deterministic jitter) with the injected spikes, and
/// unrelated background noise on the worker.
fn record_session(s: &Session) {
    let injected = spikes();
    let mut ts = 1_000 * MS;
    let mut jitter_state = 12_345u64;
    for frame in 0..1000u32 {
        jitter_state = jitter_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let jitter = (jitter_state >> 33) % MS;
        let spike = injected.iter().find(|(f, ..)| *f == frame);
        let length = spike.map_or(FRAME - MS / 2 + jitter, |(_, len, _)| *len);
        s.present.record_at(ts, EventKind::FrameBegin, frame, 0);
        s.gpu_back
            .record_at(ts + MS, EventKind::QueueSubmit, frame, 0);
        // Every critical ring records each frame, so the evidence window spans the session.
        s.gpu_front
            .record_at(ts + MS, EventKind::MacroBurst, frame, 64);
        s.worker
            .record_at(ts + 2 * MS, EventKind::CompileEnd, frame, 3 * MS);
        let injected_events = spike.map_or(&[][..], |(_, _, events)| events);
        // Injected events of one frame end 3 ms apart, so short ones do not overlap.
        for (i, (on, kind, blocked)) in (0u64..).zip(injected_events) {
            let recorder = match on {
                On::GpuBack => &s.gpu_back,
                On::GpuFront => &s.gpu_front,
                On::Worker => &s.worker,
            };
            recorder.record_at(ts + length - MS * (1 + 3 * i), *kind, frame, *blocked);
        }
        s.present
            .record_at(ts + length, EventKind::FrameEnd, frame, 0);
        ts += length;
    }
}

#[test]
fn analyzer_attributes_injected_spikes_to_their_cause() {
    let ledger = Ledger::new(4, 14);
    let session = Session {
        present: ledger.register(ThreadRole::Present).unwrap(),
        gpu_back: ledger.register(ThreadRole::GpuBack).unwrap(),
        gpu_front: ledger.register(ThreadRole::GpuFront).unwrap(),
        worker: ledger.register(ThreadRole::CompileWorker).unwrap(),
    };
    record_session(&session);
    let report = analyze(&ledger.dump(), TARGET);

    assert_eq!(report.stats.frames, 1000);
    assert!((FRAME - MS..=FRAME + MS).contains(&report.stats.p50_ns));
    assert_eq!(report.stats.max_ns, 40 * MS);
    // The five spikes are the five longest frames: P99.9 is the second longest, P99 a
    // normal frame.
    assert_eq!(report.stats.p999_ns, 35 * MS);
    assert!(report.stats.p99_ns <= FRAME + MS);
    assert!(report.spikes.iter().all(|s| s.evidence_complete));

    let found: Vec<(u32, Option<EventKind>, bool)> = report
        .spikes
        .iter()
        .map(|s| (s.frame, s.cause, s.emulator_induced))
        .collect();
    assert_eq!(
        found,
        vec![
            (100, Some(EventKind::PipelineWait), true),
            (400, Some(EventKind::CacheResolve), true),
            (600, None, false),
            (800, None, false),
            (900, Some(EventKind::FenceWait), false),
        ]
    );
    let first = report.spikes[0];
    assert_eq!(first.overrun_ns, 20 * MS);
    assert_eq!(first.cause_ns, 22 * MS);
    assert_eq!(report.spikes[4].emulator_ns, 3 * MS);
}

#[test]
fn only_the_overlapping_part_of_a_wait_counts() {
    let ledger = Ledger::new(2, 6);
    let present = ledger.register(ThreadRole::Present).unwrap();
    let back = ledger.register(ThreadRole::GpuBack).unwrap();
    present.record_at(100 * MS, EventKind::FrameBegin, 7, 0);
    // A 30 ms wait that started 20 ms before the frame: only 10 ms fall inside it.
    back.record_at(110 * MS, EventKind::PipelineWait, 7, 30 * MS);
    present.record_at(130 * MS, EventKind::FrameEnd, 7, 0);
    let report = analyze(&ledger.dump(), TARGET);
    assert_eq!(report.spikes.len(), 1);
    assert_eq!(report.spikes[0].cause_ns, 10 * MS);
    assert!(report.spikes[0].emulator_induced);
}

#[test]
fn concurrent_waits_are_counted_once() {
    let ledger = Ledger::new(4, 6);
    let present = ledger.register(ThreadRole::Present).unwrap();
    let back = ledger.register(ThreadRole::GpuBack).unwrap();
    let front = ledger.register(ThreadRole::GpuFront).unwrap();
    present.record_at(100 * MS, EventKind::FrameBegin, 3, 0);
    // Three threads wait during the same 2 ms: 2 ms of the 10 ms overrun is covered, not 6.
    back.record_at(110 * MS, EventKind::FenceWait, 3, 2 * MS);
    front.record_at(110 * MS, EventKind::PipelineWait, 3, 2 * MS);
    present.record_at(110 * MS, EventKind::FenceWait, 3, 2 * MS);
    present.record_at(130 * MS, EventKind::FrameEnd, 3, 0);
    let spike = analyze(&ledger.dump(), TARGET).spikes[0];
    assert_eq!(spike.emulator_ns, 2 * MS);
    assert!(!spike.emulator_induced);
}

#[test]
fn frames_whose_evidence_was_overwritten_are_not_blamed_on_the_game() {
    // 64-event rings: Present keeps all 10 frames, the busy GPU back thread only the last few.
    let ledger = Ledger::new(2, 6);
    let present = ledger.register(ThreadRole::Present).unwrap();
    let back = ledger.register(ThreadRole::GpuBack).unwrap();
    let mut ts = 1_000 * MS;
    for frame in 0..10u32 {
        let length = if frame == 1 { 40 * MS } else { FRAME };
        present.record_at(ts, EventKind::FrameBegin, frame, 0);
        for i in 0..20 {
            back.record_at(ts + i * 100_000, EventKind::QueueSubmit, frame, 0);
        }
        if frame == 1 {
            back.record_at(ts + 30 * MS, EventKind::PipelineWait, frame, 25 * MS);
        }
        present.record_at(ts + length, EventKind::FrameEnd, frame, 0);
        ts += length;
    }
    let report = analyze(&ledger.dump(), TARGET);
    let spike = report.spikes[0];
    assert_eq!(spike.frame, 1);
    assert!(
        !spike.evidence_complete,
        "frame 1's GPU events were overwritten"
    );
    assert_eq!(spike.cause, None);
    assert!(!spike.emulator_induced);
    assert_eq!(report.spikes.len(), 1);
}
