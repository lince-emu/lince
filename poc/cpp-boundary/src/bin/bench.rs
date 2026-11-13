// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-11 call-cost benchmark: nanoseconds per call for
//! - a non-inlined Rust call (baseline),
//! - a Rust → C++ call through the C ABI (`lb_add`),
//! - a C++ → Rust callback (`lb_sum_range` calling `read_u32` once per word).
//!
//! `bench [--calls N] [--runs R]`; prints the median and the range of `R` runs.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line tool: results go to the terminal"
)]

use std::hint::black_box;
use std::process::ExitCode;
use std::time::Instant;

use poc_cpp_boundary::{Memory, add, sum_range};

#[inline(never)]
fn rust_add(a: u32, b: u32) -> u32 {
    a.wrapping_add(b)
}

/// Returns the low address bits, so every call does a little work the optimizer cannot skip.
struct AddressMemory;

impl Memory for AddressMemory {
    #[inline(never)]
    fn read_u32(&mut self, addr: u64) -> u32 {
        (addr & 0xFFFF_FFFF).try_into().unwrap_or(0)
    }
}

/// Mean nanoseconds per call of `calls` calls of `f`.
fn per_call(calls: u32, mut f: impl FnMut(u32) -> u32) -> f64 {
    let start = Instant::now();
    let mut acc = 0u32;
    for i in 0..calls {
        acc = acc.wrapping_add(f(black_box(i)));
    }
    black_box(acc);
    start.elapsed().as_secs_f64() * 1e9 / f64::from(calls)
}

fn callback_per_call(calls: u32) -> f64 {
    let mut memory = AddressMemory;
    let start = Instant::now();
    let sum = sum_range(&mut memory, black_box(0), calls, 4);
    let elapsed = start.elapsed();
    black_box(sum.ok());
    elapsed.as_secs_f64() * 1e9 / f64::from(calls)
}

fn summary(name: &str, mut ns: Vec<f64>) {
    ns.sort_by(f64::total_cmp);
    let median = ns[ns.len() / 2];
    println!(
        "{name:<26} median {median:6.2} ns/call   (min {:5.2}, max {:5.2}, {} runs)",
        ns[0],
        ns[ns.len() - 1],
        ns.len()
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str, default: u32| -> Option<u32> {
        match args.iter().position(|a| a == name) {
            Some(i) => args.get(i + 1)?.parse().ok().filter(|&n| n > 0),
            None => Some(default),
        }
    };
    let (Some(calls), Some(runs)) = (flag("--calls", 100_000_000), flag("--runs", 7)) else {
        eprintln!("usage: bench [--calls N] [--runs R]  (positive integers)");
        return ExitCode::FAILURE;
    };
    // One untimed run of each, so code and data are warm.
    per_call(calls / 10, |i| rust_add(i, 1));
    per_call(calls / 10, |i| add(i, 1));
    callback_per_call(calls / 10);
    let collect = |f: &dyn Fn() -> f64| (0..runs).map(|_| f()).collect::<Vec<_>>();
    summary(
        "rust non-inlined call",
        collect(&|| per_call(calls, |i| rust_add(i, 1))),
    );
    summary(
        "rust -> c++ (lb_add)",
        collect(&|| per_call(calls, |i| add(i, 1))),
    );
    summary(
        "c++ -> rust callback",
        collect(&|| callback_per_call(calls)),
    );
    ExitCode::SUCCESS
}
