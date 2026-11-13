// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-5 harness: pipeline-creation latency per mechanism, program size and worker count.
//!
//! `harness [--ops 16,1024,16384] [--samples 20] [--workers 1,2,4,8] [--validation]
//!          [--nonce N] [--out results/pipeline_latency.csv]`
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line tool: results go to the terminal"
)]

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use poc_vk_pipelines::{Context, Mechanism, Validation, Worker};

type Result<T> = std::result::Result<T, String>;

/// One CSV row.
struct Row {
    mechanism: Mechanism,
    ops: u32,
    workers: u32,
    worker: u32,
    sample: u32,
    ns: u64,
}

struct Options {
    ops: Vec<u32>,
    samples: u32,
    workers: Vec<u32>,
    validation: bool,
    nonce: u64,
    out: PathBuf,
}

fn main() -> ExitCode {
    match parse(&std::env::args().skip(1).collect::<Vec<_>>()).and_then(|o| run(&o)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

fn list(value: &str, name: &str) -> Result<Vec<u32>> {
    let items: Vec<u32> = value
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| format!("{name}: expected comma-separated positive numbers"))?;
    if items.is_empty() || items.contains(&0) {
        return Err(format!("{name}: values must be positive"));
    }
    Ok(items)
}

fn parse(args: &[String]) -> Result<Options> {
    let default_nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
    let mut o = Options {
        ops: vec![16, 1024, 16_384],
        samples: 20,
        workers: vec![1, 2, 4, 8],
        validation: false,
        nonce: default_nonce,
        out: PathBuf::from("results/pipeline_latency.csv"),
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        if flag == "--validation" {
            o.validation = true;
            continue;
        }
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--ops" => o.ops = list(value, flag)?,
            "--workers" => o.workers = list(value, flag)?,
            "--samples" => {
                o.samples = value
                    .parse()
                    .ok()
                    .filter(|&n| n > 0)
                    .ok_or("--samples: expected one positive number")?;
            }
            "--nonce" => o.nonce = value.parse().map_err(|_| "--nonce: not a number")?,
            "--out" => o.out = PathBuf::from(value),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(o)
}

/// The 64-bit FNV prime, used to mix the seed parts.
const FNV_PRIME: u64 = 0x100_0000_01B3;

/// CSV header; `nonce` tells runs appended to the same file apart.
const CSV_HEADER: &str = "nonce,device,driver,mechanism,ops,workers,worker,sample,ns";

/// Seed unique per run (nonce), worker count, worker, size and sample, so no driver cache
/// (in memory or on disk) has seen the program before.
fn seed(o: &Options, workers: u32, worker: u32, size: usize, sample: u32) -> u64 {
    let parts = [
        u64::from(workers),
        u64::from(worker),
        size as u64,
        u64::from(sample),
    ];
    parts.iter().fold(o.nonce, |acc, &p| {
        acc.wrapping_mul(FNV_PRIME).wrapping_add(p)
    })
}

/// One worker thread: an untimed warm-up sample, then `samples` per size.
fn work(ctx: &Context, o: &Options, workers: u32, worker: u32) -> Result<Vec<Row>> {
    let mut w = Worker::new(ctx, o.nonce).map_err(|e| e.to_string())?;
    w.sample(o.ops[0], seed(o, workers, worker, usize::MAX, 0))
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for (size, &ops) in o.ops.iter().enumerate() {
        for sample in 0..o.samples {
            let timings = w
                .sample(ops, seed(o, workers, worker, size, sample))
                .map_err(|e| e.to_string())?;
            rows.extend(timings.into_iter().map(|t| Row {
                mechanism: t.mechanism,
                ops,
                workers,
                worker,
                sample,
                ns: t.ns,
            }));
        }
    }
    Ok(rows)
}

fn run(o: &Options) -> Result<()> {
    let validation = if o.validation {
        Validation::Enabled
    } else {
        Validation::Disabled
    };
    let ctx = Context::new(validation).map_err(|e| e.to_string())?;
    let caps = ctx.caps();
    println!("device: {} | driver: {}", caps.device_name, caps.driver);
    println!(
        "graphics_pipeline_library: {} (fast linking: {}) | shader_object: {} | validation: {} | nonce: {}",
        caps.graphics_pipeline_library,
        caps.gpl_fast_linking,
        caps.shader_object,
        ctx.validation_enabled(),
        o.nonce
    );
    let shared = &ctx;
    for &workers in &o.workers {
        let rows = std::thread::scope(|s| {
            let handles: Vec<_> = (0..workers)
                .map(|worker| s.spawn(move || work(shared, o, workers, worker)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().map_err(|_| "worker panicked".to_owned())?)
                .collect::<Result<Vec<Vec<Row>>>>()
        })?;
        let rows: Vec<Row> = rows.into_iter().flatten().collect();
        write_csv(&o.out, o.nonce, &ctx, &rows)?;
        summarize(workers, &rows);
    }
    if ctx.validation_enabled() {
        println!(
            "validation errors and warnings: {}",
            ctx.validation_messages()
        );
    }
    drop(ctx);
    println!("wrote {}", o.out.display());
    Ok(())
}

fn write_csv(path: &Path, nonce: u64, ctx: &Context, rows: &[Row]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let fresh = !path.exists();
    if !fresh {
        let header = BufReader::new(File::open(path).map_err(|e| e.to_string())?)
            .lines()
            .next()
            .transpose()
            .map_err(|e| e.to_string())?;
        if header.as_deref() != Some(CSV_HEADER) {
            return Err(format!(
                "{}: different CSV columns; move the file away or pass --out",
                path.display()
            ));
        }
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let io = |e: std::io::Error| e.to_string();
    if fresh {
        writeln!(f, "{CSV_HEADER}").map_err(io)?;
    }
    let (device, driver) = (
        ctx.caps().device_name.replace(',', " "),
        ctx.caps().driver.replace(',', " "),
    );
    for r in rows {
        writeln!(
            f,
            "{nonce},{device},{driver},{},{},{},{},{},{}",
            r.mechanism.name(),
            r.ops,
            r.workers,
            r.worker,
            r.sample,
            r.ns
        )
        .map_err(io)?;
    }
    Ok(())
}

fn percentile(sorted: &[u64], per_mille: usize) -> u64 {
    let rank = (sorted.len() * per_mille).div_ceil(1000).max(1);
    sorted[rank - 1]
}

/// Prints P50 / P99 / max per mechanism and size, in microseconds.
fn summarize(workers: u32, rows: &[Row]) {
    let mut groups: BTreeMap<(Mechanism, u32), Vec<u64>> = BTreeMap::new();
    for r in rows {
        groups.entry((r.mechanism, r.ops)).or_default().push(r.ns);
    }
    println!("workers = {workers}");
    for ((mechanism, ops), mut ns) in groups {
        ns.sort_unstable();
        let us = |v: u64| v / 1000;
        println!(
            "  {:<32} ops {:>6}: p50 {:>8} us  p99 {:>8} us  max {:>8} us  (n = {})",
            mechanism.name(),
            ops,
            us(percentile(&ns, 500)),
            us(percentile(&ns, 990)),
            us(ns[ns.len() - 1]),
            ns.len()
        );
    }
}
