// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimal analyzer: frame-time percentiles and spike attribution
//! (docs/05-performance-validation.md §4).
//!
//! A frame spans its earliest `FrameBegin` and latest `FrameEnd`. A frame is a spike when it is
//! longer than the target. Its overrun is attributed to events with a blocking duration
//! (`EventKind::blocking_ns`) recorded on render-critical threads. Each event covers
//! `[ts − duration, ts]` clipped to the frame; overlapping intervals (concurrent waits on several
//! threads, nested events) are merged, so time is counted once. The cause is the kind covering
//! the most time. A spike is emulator-induced when the merged coverage is at least half of the
//! overrun (doc 05 §4).
//!
//! Rings overwrite their oldest events at different rates, so a dump only holds complete
//! evidence from the point where every render-critical ring that lost events still has its
//! history. Spikes before that point are reported with `evidence_complete = false` and no
//! verdict, rather than being blamed on the game. Not handled: events recorded after a ring
//! was snapshotted (`Ledger::dump` copies rings one after another), which can only affect the
//! last frames of a dump.
//!
//! Not covered: classes of doc 05 §4 that need more than blocking durations (allocations,
//! uploads and evictions by size, shader discovery, thread busy ≥ 95 %, present delay,
//! guest-core idle time, reproducibility for "game's own behavior").

use std::collections::HashMap;

use crate::dump::Dump;
use crate::event::EventKind;

/// Frame-time distribution, nearest-rank percentiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameStats {
    /// Complete frames (both begin and end seen).
    pub frames: usize,
    /// Median frame time, ns.
    pub p50_ns: u64,
    /// 99th percentile, ns.
    pub p99_ns: u64,
    /// 99.9th percentile, ns.
    pub p999_ns: u64,
    /// Longest frame, ns.
    pub max_ns: u64,
}

/// One frame over target and what covered it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spike {
    /// Frame sequence number.
    pub frame: u32,
    /// Frame time, ns.
    pub frame_ns: u64,
    /// Frame time minus target, ns.
    pub overrun_ns: u64,
    /// Every render-critical ring still held events from the whole frame. When false, the
    /// fields below are empty: the evidence was overwritten.
    pub evidence_complete: bool,
    /// The event kind covering most of the frame on critical threads, if any.
    pub cause: Option<EventKind>,
    /// Nanoseconds covered by `cause` (overlaps merged).
    pub cause_ns: u64,
    /// Nanoseconds covered by all emulator events on critical threads (overlaps merged).
    pub emulator_ns: u64,
    /// `emulator_ns` covers at least half of the overrun.
    pub emulator_induced: bool,
}

/// Analyzer output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// Distribution of all complete frames.
    pub stats: FrameStats,
    /// Frames over target, in frame order.
    pub spikes: Vec<Spike>,
}

/// Interval `[begin, end]` of each complete frame, in frame order.
fn frame_intervals(dump: &Dump) -> Vec<(u32, u64, u64)> {
    let mut spans: HashMap<u32, (Option<u64>, Option<u64>)> = HashMap::new();
    for e in dump.threads.iter().flat_map(|t| &t.events) {
        let span = spans.entry(e.frame).or_default();
        if e.kind == EventKind::FrameBegin {
            span.0 = Some(span.0.map_or(e.ts_ns, |b| b.min(e.ts_ns)));
        } else if e.kind == EventKind::FrameEnd {
            span.1 = Some(span.1.map_or(e.ts_ns, |b| b.max(e.ts_ns)));
        }
    }
    let mut frames: Vec<(u32, u64, u64)> = spans
        .into_iter()
        .filter_map(|(frame, span)| match span {
            (Some(begin), Some(end)) if end >= begin => Some((frame, begin, end)),
            _ => None,
        })
        .collect();
    frames.sort_unstable_by_key(|&(frame, ..)| frame);
    frames
}

/// Earliest time from which every render-critical ring still holds its full history: the
/// latest "oldest event" among critical rings that lost events. Rings that lost nothing, even
/// if sparse, do not limit it.
fn evidence_start(dump: &Dump) -> u64 {
    dump.threads
        .iter()
        .filter(|t| t.role.is_render_critical() && t.lost_events > 0)
        .map(|t| t.events.iter().map(|e| e.ts_ns).min().unwrap_or(u64::MAX))
        .max()
        .unwrap_or(0)
}

fn percentile(sorted: &[u64], per_mille: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() * per_mille).div_ceil(1000).max(1);
    sorted[rank - 1]
}

/// Total length of the union of intervals.
fn merged_len(mut intervals: Vec<(u64, u64)>) -> u64 {
    intervals.sort_unstable();
    let mut total = 0u64;
    let mut current: Option<(u64, u64)> = None;
    for (start, end) in intervals {
        current = match current {
            Some((s, e)) if start <= e => Some((s, e.max(end))),
            Some((s, e)) => {
                total = total.saturating_add(e - s);
                Some((start, end))
            }
            None => Some((start, end)),
        };
    }
    current.map_or(total, |(s, e)| total.saturating_add(e - s))
}

/// Intervals of blocking time on critical threads clipped to `[begin, end]`, by kind.
fn blocking_intervals(dump: &Dump, begin: u64, end: u64) -> HashMap<EventKind, Vec<(u64, u64)>> {
    let mut by_kind: HashMap<EventKind, Vec<(u64, u64)>> = HashMap::new();
    let critical = dump.threads.iter().filter(|t| t.role.is_render_critical());
    for e in critical.flat_map(|t| &t.events) {
        let Some(duration) = e.kind.blocking_ns(e.arg) else {
            continue;
        };
        let start = e.ts_ns.saturating_sub(duration).max(begin);
        let stop = e.ts_ns.min(end);
        if stop > start {
            by_kind.entry(e.kind).or_default().push((start, stop));
        }
    }
    by_kind
}

fn spike(dump: &Dump, evidence_from: u64, frame: (u32, u64, u64), target: u64) -> Spike {
    let (number, begin, end) = frame;
    let overrun_ns = end - begin - target;
    let evidence_complete = evidence_from <= begin;
    let mut spike = Spike {
        frame: number,
        frame_ns: end - begin,
        overrun_ns,
        evidence_complete,
        cause: None,
        cause_ns: 0,
        emulator_ns: 0,
        emulator_induced: false,
    };
    if !evidence_complete {
        return spike;
    }
    let by_kind = blocking_intervals(dump, begin, end);
    spike.emulator_ns = merged_len(by_kind.values().flatten().copied().collect());
    let top = by_kind
        .into_iter()
        .map(|(kind, intervals)| (kind, merged_len(intervals)))
        .max_by_key(|&(kind, ns)| (ns, std::cmp::Reverse(kind.to_raw())));
    spike.cause = top.map(|(kind, _)| kind);
    spike.cause_ns = top.map_or(0, |(_, ns)| ns);
    spike.emulator_induced = spike.emulator_ns >= overrun_ns.div_ceil(2);
    spike
}

/// Analyzes a dump against a frame-time target.
pub fn analyze(dump: &Dump, target_ns: u64) -> Report {
    let frames = frame_intervals(dump);
    let mut times: Vec<u64> = frames.iter().map(|&(_, b, e)| e - b).collect();
    times.sort_unstable();
    let stats = FrameStats {
        frames: frames.len(),
        p50_ns: percentile(&times, 500),
        p99_ns: percentile(&times, 990),
        p999_ns: percentile(&times, 999),
        max_ns: times.last().copied().unwrap_or(0),
    };
    let evidence_from = evidence_start(dump);
    let spikes = frames
        .iter()
        .filter(|&&(_, begin, end)| end - begin > target_ns)
        .map(|&frame| spike(dump, evidence_from, frame, target_ns))
        .collect();
    Report { stats, spikes }
}

#[cfg(test)]
mod tests {
    use super::merged_len;

    #[test]
    fn merges_overlapping_and_touching_intervals() {
        assert_eq!(merged_len(vec![]), 0);
        assert_eq!(merged_len(vec![(0, 10), (5, 15), (15, 20), (30, 31)]), 21);
        assert_eq!(merged_len(vec![(0, 10), (2, 3)]), 10);
    }
}
