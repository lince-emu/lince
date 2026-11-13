// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Event-cost microbenchmark (PoC-7 success criterion: ≤ 30 ns per event).
//!
//! `bench [--events N] [--runs R] [--out results/event_cost.csv]`
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line tool: results go to the terminal"
)]

use std::fs::{self, OpenOptions};
use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use poc_frame_ledger::{EventKind, Ledger, ThreadRole};

/// Ring size used by the benchmark: 2^16 events per thread.
const CAPACITY_LOG2: u32 = 16;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let events: u64 = flag(&args, "--events")
        .map_or(Ok(10_000_000), |v| v.parse())
        .map_err(|_| "--events: not a number")?;
    let runs: u32 = flag(&args, "--runs")
        .map_or(Ok(5), |v| v.parse())
        .map_err(|_| "--runs: not a number")?;
    if events == 0 || runs == 0 {
        return Err("--events and --runs must be positive".to_owned());
    }
    let out =
        PathBuf::from(flag(&args, "--out").unwrap_or_else(|| "results/event_cost.csv".to_owned()));
    for run in 0..runs {
        for (scenario, ns) in [
            ("record", measure(events, Clock::Now, false)),
            ("record_at", measure(events, Clock::Fixed, false)),
            ("record_with_dumper", measure(events, Clock::Now, true)),
        ] {
            // Picoseconds per event, integer arithmetic (no float casts).
            let ps = u128::from(ns) * 1000 / u128::from(events);
            let per_event = format!("{}.{:03}", ps / 1000, ps % 1000);
            println!("run {run} {scenario}: {per_event} ns/event");
            append_csv(&out, &format!("{scenario},{run},{events},{ns},{per_event}"))?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Clock {
    /// `record`: reads the monotonic clock per event.
    Now,
    /// `record_at`: fixed timestamp, isolates the ring cost.
    Fixed,
}

/// Total nanoseconds to record `events` events on one thread, optionally while another thread
/// dumps the ledger in a loop.
fn measure(events: u64, clock: Clock, with_dumper: bool) -> u64 {
    let ledger = Ledger::new(2, CAPACITY_LOG2);
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        if with_dumper {
            s.spawn(|| dump_until(&ledger, &stop));
        }
        let recorder = ledger
            .register(ThreadRole::GpuBack)
            .unwrap_or_else(|| unreachable!("a fresh two-slot ledger has a free slot"));
        let start = Instant::now();
        for i in 0..events {
            let frame = black_box(u32::try_from(i >> 10).unwrap_or(u32::MAX));
            match clock {
                Clock::Now => recorder.record(black_box(EventKind::QueueSubmit), frame, i),
                Clock::Fixed => recorder.record_at(i, black_box(EventKind::QueueSubmit), frame, i),
            }
        }
        let ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        stop.store(true, Ordering::Relaxed);
        ns
    })
}

/// Dumps the ledger in a loop until `stop`, as an always-on reader would.
fn dump_until(ledger: &Ledger, stop: &AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        black_box(ledger.dump());
    }
}

fn append_csv(path: &Path, row: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let fresh = !path.exists();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    if fresh {
        writeln!(f, "scenario,run,events,total_ns,ns_per_event").map_err(|e| e.to_string())?;
    }
    writeln!(f, "{row}").map_err(|e| e.to_string())
}
