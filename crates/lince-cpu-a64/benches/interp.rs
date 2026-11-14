// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tier A interpreter throughput on the B1 programs (T-043, ADR-0005 validation plan).
//!
//! Each B1 program in `tests/homebrew/` covers one instruction group. A round loads `SPACES` x
//! `COPIES_PER_SPACE` copies of a program at distinct bases (untimed; an address space holds
//! at most 64 regions), then times running every copy from its
//! entry to `ExitProcess`, SVC round trips included. Fresh copies keep the program's starting
//! memory (a zeroed bss) without resetting it inside the timed loop. The result is the time per
//! completed guest instruction; the minimum, median and maximum over `ROUNDS` rounds are
//! printed.
//!
//! Run with `cargo bench -p lince-cpu-a64 --bench interp`.

use std::error::Error;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use lince_common::GuestVa;
use lince_cpu_a64::{Exit, State, run};
use lince_loader::load_nro;
use lince_mem::{AddressSpace, SpaceConfig};

/// The instruction group and the program that covers it.
const PROGRAMS: [(&str, &str); 4] = [
    ("exit (hello)", "hello"),
    ("integer (alu)", "alu"),
    ("branch (branch)", "branch"),
    ("load/store (memory)", "memory"),
];
const SPACES: usize = 32;
const COPIES_PER_SPACE: u64 = 8;
const ROUNDS: usize = 15;
const BASE: u64 = 0x800_0000;
/// Distance between copies; every B1 image is far smaller.
const STRIDE: u64 = 0x10_0000;
const OUTPUT_DEBUG_STRING: u16 = 0x27;
const EXIT_PROCESS: u16 = 0x07;
/// Far more instructions than any B1 program runs; reaching it means it never exited.
const LIMIT: u64 = 10_000;

type BenchResult<T> = Result<T, Box<dyn Error>>;

/// One copy's run: instructions completed and the last `OutputDebugString` length.
struct CopyRun {
    executed: u64,
    printed_len: u64,
}

fn main() -> BenchResult<()> {
    let mut out = io::stdout().lock();
    writeln!(
        out,
        "Tier A interpreter, {} copies x {ROUNDS} rounds per program",
        SPACES as u64 * COPIES_PER_SPACE
    )?;
    writeln!(
        out,
        "{:<22}{:>10}{:>12}{:>12}{:>12}{:>12}",
        "group (program)", "insns/run", "min ns/i", "median ns/i", "max ns/i", "median MIPS"
    )?;
    for (group, name) in PROGRAMS {
        let file = std::fs::read(format!(
            "{}/../../tests/homebrew/{name}.nro",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        let (per_run, mut samples) = measure(&file)?;
        samples.sort_by(f64::total_cmp);
        let median = samples[samples.len() / 2];
        writeln!(
            out,
            "{group:<22}{per_run:>10}{:>12.2}{median:>12.2}{:>12.2}{:>12.0}",
            samples[0],
            samples[samples.len() - 1],
            1_000.0 / median
        )?;
    }
    Ok(())
}

/// Returns the instructions one run completes and the ns per instruction of each round.
fn measure(file: &[u8]) -> BenchResult<(u64, Vec<f64>)> {
    let mut per_run = None;
    let mut samples = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let mut spaces = (0..SPACES)
            .map(|_| load_copies(file))
            .collect::<BenchResult<Vec<_>>>()?;

        let start = Instant::now();
        let mut runs = Vec::new();
        for (mem, states) in &mut spaces {
            for state in states {
                runs.push(run_to_exit(state, mem)?);
            }
        }
        let elapsed = start.elapsed();

        let executed = check_runs(&runs, &mut per_run)?;
        samples.push(ns_per_insn(elapsed, executed));
    }
    Ok((per_run.unwrap_or(0), samples))
}

/// An address space with `COPIES_PER_SPACE` copies of `file` and a state at each entry.
fn load_copies(file: &[u8]) -> BenchResult<(AddressSpace, Vec<State>)> {
    let mut mem = AddressSpace::new(SpaceConfig::for_host())?;
    let mut states = Vec::new();
    for i in 0..COPIES_PER_SPACE {
        let nro = load_nro(&mut mem, GuestVa::new(BASE + i * STRIDE), file)?;
        states.push(State {
            pc: nro.entry.get(),
            ..State::default()
        });
    }
    Ok((mem, states))
}

fn run_to_exit(state: &mut State, mem: &mut AddressSpace) -> BenchResult<CopyRun> {
    let mut executed = 0;
    let mut printed_len = 0;
    loop {
        let result = run(state, mem, LIMIT)?;
        executed += result.executed;
        match result.exit {
            Exit::Svc {
                number: OUTPUT_DEBUG_STRING,
            } => printed_len = state.x[1],
            Exit::Svc {
                number: EXIT_PROCESS,
            } => {
                return Ok(CopyRun {
                    executed,
                    printed_len,
                });
            }
            other @ (Exit::Svc { .. }
            | Exit::Limit
            | Exit::Undefined { .. }
            | Exit::Unimplemented { .. }
            | Exit::FetchAbort { .. }
            | Exit::PcMisaligned { .. }
            | Exit::DataAbort { .. }) => {
                return Err(format!("unexpected exit {other:?} at pc {:#x}", state.pc).into());
            }
        }
    }
}

/// Checks that every copy ran the same path and did not print "fail" (length 4); returns the
/// total instructions of the round.
fn check_runs(runs: &[CopyRun], per_run: &mut Option<u64>) -> BenchResult<u64> {
    let mut total = 0;
    for copy in runs {
        if copy.printed_len == 4 {
            return Err("a self-checking program printed \"fail\"".into());
        }
        if *per_run.get_or_insert(copy.executed) != copy.executed {
            return Err("copies of one program ran different instruction counts".into());
        }
        total += copy.executed;
    }
    Ok(total)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "nanosecond and instruction counts of one round fit in f64's 52-bit mantissa"
)]
fn ns_per_insn(elapsed: Duration, executed: u64) -> f64 {
    elapsed.as_nanos() as f64 / executed as f64
}
