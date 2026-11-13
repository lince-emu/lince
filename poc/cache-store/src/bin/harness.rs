// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-6 measurement harness. See `poc/cache-store/README.md` for usage.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line tool: progress and errors go to the terminal"
)]

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use poc_cache_store::workload::{self, SizeDist, SplitMix64, Workload};
use poc_cache_store::{BlobStore, Engine, custom::CustomStore, open_store};

type Result<T> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("open-time") => parse_flags(&args[1..]).and_then(|f| open_time(&f)),
        Some("resolve") => parse_flags(&args[1..]).and_then(|f| resolve(&f)),
        Some("append-forever") => parse_flags(&args[1..]).and_then(|f| append_forever(&f)),
        _ => Err(USAGE.to_owned()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = "usage:
  harness open-time --engine E --dir D --sizes 1M,64M,1G [--runs 20] [--seed 1] [--dist D] [--out F]
  harness resolve   --engine E --dir D --size 256M [--lookups 10000] [--writer on|off] [--seed 1] [--dist D] [--out F]
  harness append-forever --dir D [--seed 1] [--dist D] [--limit N]
engines: custom, redb, sqlite (needs --features sqlite); dist: loguniform:MIN:MAX | uniform:MIN:MAX";

struct Flags(HashMap<String, String>);

fn parse_flags(args: &[String]) -> Result<Flags> {
    let mut map = HashMap::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let name = flag
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument `{flag}`"))?;
        let value = it
            .next()
            .ok_or_else(|| format!("missing value for --{name}"))?;
        map.insert(name.to_owned(), value.clone());
    }
    Ok(Flags(map))
}

impl Flags {
    fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    fn required(&self, name: &str) -> Result<&str> {
        self.get(name)
            .ok_or_else(|| format!("--{name} is required\n{USAGE}"))
    }

    fn number(&self, name: &str, default: u64) -> Result<u64> {
        self.get(name).map_or(Ok(default), |v| {
            v.parse()
                .map_err(|_| format!("--{name}: not a number: {v}"))
        })
    }

    fn engine(&self) -> Result<Engine> {
        let name = self.required("engine")?;
        Engine::parse(name).ok_or_else(|| format!("unknown engine `{name}`"))
    }

    fn dist(&self) -> Result<SizeDist> {
        self.get("dist").map_or(Ok(SizeDist::DEFAULT), |v| {
            SizeDist::parse(v).ok_or_else(|| format!("bad --dist `{v}`"))
        })
    }

    fn out(&self, default: &str) -> PathBuf {
        PathBuf::from(self.get("out").unwrap_or(default))
    }
}

/// Parses `512K`, `64M`, `4G` or a plain byte count.
fn parse_size(s: &str) -> Result<u64> {
    let (digits, shift) = match s.as_bytes().last() {
        Some(b'K' | b'k') => (&s[..s.len() - 1], 10),
        Some(b'M' | b'm') => (&s[..s.len() - 1], 20),
        Some(b'G' | b'g') => (&s[..s.len() - 1], 30),
        _ => (s, 0),
    };
    let n: u64 = digits.parse().map_err(|_| format!("bad size `{s}`"))?;
    n.checked_mul(1 << shift)
        .ok_or_else(|| format!("size `{s}` is too large"))
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// A populated store directory: the base workload plus whatever earlier `--writer on` runs
/// added (those keys are not looked up, but they are on disk).
struct Population {
    dir: PathBuf,
    keys: Vec<u128>,
    meta: Meta,
}

/// Contents of `workload.meta`, fields separated by `|`.
struct Meta {
    params: String,
    base_items: usize,
    writer_runs: u64,
    extra_items: u64,
    extra_bytes: u64,
}

impl Meta {
    fn parse(text: &str) -> Option<Self> {
        let mut f = text.trim().split('|');
        let meta = Self {
            params: f.next()?.to_owned(),
            base_items: f.next()?.parse().ok()?,
            writer_runs: f.next()?.parse().ok()?,
            extra_items: f.next()?.parse().ok()?,
            extra_bytes: f.next()?.parse().ok()?,
        };
        f.next().is_none().then_some(meta)
    }

    fn write(&self, dir: &Path) -> Result<()> {
        let text = format!(
            "{}|{}|{}|{}|{}",
            self.params, self.base_items, self.writer_runs, self.extra_items, self.extra_bytes
        );
        fs::write(dir.join("workload.meta"), text).map_err(|e| e.to_string())
    }
}

impl Population {
    /// Items actually stored, including earlier background writes.
    fn items_on_disk(&self) -> u64 {
        self.keys.len() as u64 + self.meta.extra_items
    }

    /// Payload bytes actually stored, including earlier background writes.
    fn bytes_on_disk(&self) -> u64 {
        let base: u64 = self
            .keys
            .iter()
            .map(|&k| workload::payload_len(k) as u64)
            .sum();
        base + self.meta.extra_bytes
    }
}

/// Fills `<dir>/<engine>-<target>` with workload items until `target` payload bytes, or
/// reuses it if a previous run with the same parameters already did.
fn populate(
    engine: Engine,
    root: &Path,
    target: u64,
    seed: u64,
    dist: SizeDist,
) -> Result<Population> {
    let dir = root.join(format!("{}-{target}", engine.name()));
    let params = format!("{seed} {dist:?}");
    match fs::read_to_string(dir.join("workload.meta")) {
        Ok(text) => {
            let meta = Meta::parse(&text).ok_or("corrupt workload.meta")?;
            if meta.params != params {
                return Err(format!(
                    "{} holds a different workload; use another --dir",
                    dir.display()
                ));
            }
            let keys = generate_keys(seed, dist, meta.base_items);
            return Ok(Population { dir, keys, meta });
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("read {}: {e}", dir.display())),
    }
    let mut store = open_store(engine, &dir).map_err(err)?;
    let mut w = Workload::new(seed, dist);
    let (mut bytes, mut items) = (0u64, 0usize);
    while bytes < target {
        let key = w.next_key();
        store.put(key, 1, &workload::payload(key)).map_err(err)?;
        bytes += workload::payload_len(key) as u64;
        items += 1;
        if items.is_multiple_of(64) {
            store.flush().map_err(err)?;
        }
    }
    store.flush().map_err(err)?;
    let meta = Meta {
        params,
        base_items: items,
        writer_runs: 0,
        extra_items: 0,
        extra_bytes: 0,
    };
    meta.write(&dir)?;
    let keys = generate_keys(seed, dist, items);
    Ok(Population { dir, keys, meta })
}

fn generate_keys(seed: u64, dist: SizeDist, items: usize) -> Vec<u128> {
    let mut w = Workload::new(seed, dist);
    (0..items).map(|_| w.next_key()).collect()
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn append_csv(path: &Path, header: &str, row: &str) -> Result<()> {
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
        writeln!(f, "{header}").map_err(|e| e.to_string())?;
    }
    writeln!(f, "{row}").map_err(|e| e.to_string())
}

/// Open time per engine and store size, warm OS cache.
fn open_time(f: &Flags) -> Result<()> {
    let engine = f.engine()?;
    let root = PathBuf::from(f.required("dir")?);
    let runs = f.number("runs", 20)?;
    let (seed, dist) = (f.number("seed", 1)?, f.dist()?);
    let out = f.out("results/open_time.csv");
    for size in f.required("sizes")?.split(',') {
        let target = parse_size(size)?;
        let pop = populate(engine, &root, target, seed, dist)?;
        for run in 0..runs {
            let start = Instant::now();
            let store = open_store(engine, &pop.dir).map_err(err)?;
            let ns = elapsed_ns(start);
            drop(store);
            let row = format!(
                "{},{target},{},{},{run},{ns}",
                engine.name(),
                pop.bytes_on_disk(),
                pop.items_on_disk()
            );
            append_csv(
                &out,
                "engine,target_bytes,stored_bytes,items,run,open_ns",
                &row,
            )?;
        }
        println!(
            "{} {size}: {} items, {runs} runs",
            engine.name(),
            pop.items_on_disk()
        );
    }
    Ok(())
}

/// Nearest-rank percentile of sorted samples, `per_mille` in 1..=1000.
fn percentile(sorted: &[u64], per_mille: usize) -> u64 {
    let rank = (sorted.len() * per_mille).div_ceil(1000).max(1);
    sorted[rank - 1]
}

/// Resolve latency (lookup + read + validate) of random existing keys, optionally while
/// another thread appends new artifacts.
fn resolve(f: &Flags) -> Result<()> {
    let engine = f.engine()?;
    let root = PathBuf::from(f.required("dir")?);
    let target = parse_size(f.required("size")?)?;
    let lookups = usize::try_from(f.number("lookups", 10_000)?)
        .ok()
        .filter(|&n| n > 0)
        .ok_or("--lookups must be between 1 and usize::MAX")?;
    let (seed, dist) = (f.number("seed", 1)?, f.dist()?);
    let with_writer = f.get("writer") == Some("on");
    let mut pop = populate(engine, &root, target, seed, dist)?;
    // Every writer-on run appends keys no earlier run used, so each engine does real writes.
    let writer_seed = seed ^ 0xFEED ^ (pop.meta.writer_runs << 32);
    let mut writer = open_store(engine, &pop.dir).map_err(err)?;
    let mut reader = writer.reader().map_err(err)?;
    let stop = AtomicBool::new(false);
    let (samples, misses, background) = std::thread::scope(|s| {
        let handle =
            with_writer.then(|| s.spawn(|| write_until(&mut *writer, writer_seed, dist, &stop)));
        let measured = measure_lookups(&mut *reader, &pop.keys, lookups, seed);
        stop.store(true, Ordering::Relaxed);
        let background = match handle {
            Some(h) => h
                .join()
                .map_err(|_| "writer thread panicked".to_owned())??,
            None => (0, 0),
        };
        measured.map(|(samples, misses)| (samples, misses, background))
    })?;
    if with_writer {
        pop.meta.writer_runs += 1;
        pop.meta.extra_items += background.0;
        pop.meta.extra_bytes += background.1;
        pop.meta.write(&pop.dir)?;
    }
    let mut sorted = samples;
    sorted.sort_unstable();
    let row = format!(
        "{},{target},{},{},{},{lookups},{},{},{},{},{misses}",
        engine.name(),
        pop.items_on_disk(),
        if with_writer { "on" } else { "off" },
        background.0,
        percentile(&sorted, 500),
        percentile(&sorted, 990),
        percentile(&sorted, 999),
        sorted.last().copied().unwrap_or(0),
    );
    let header = "engine,target_bytes,items_on_disk,writer,background_puts,lookups,\
                  p50_ns,p99_ns,p999_ns,max_ns,misses";
    append_csv(&f.out("results/resolve.csv"), header, &row)?;
    println!("{row}");
    Ok(())
}

fn measure_lookups(
    reader: &mut dyn BlobStore,
    keys: &[u128],
    lookups: usize,
    seed: u64,
) -> Result<(Vec<u64>, u64)> {
    let mut rng = SplitMix64::new(seed ^ 0xA5A5);
    let mut samples = Vec::with_capacity(lookups);
    let mut misses = 0u64;
    let count = u64::try_from(keys.len()).map_err(|_| "too many keys")?;
    for _ in 0..lookups {
        let key = keys[usize::try_from(rng.below(count)).map_err(|_| "index")?];
        let start = Instant::now();
        let blob = reader.get(key).map_err(err)?;
        samples.push(elapsed_ns(start));
        match blob {
            Some(b) if workload::verify(key, &b) => {}
            Some(_) => return Err(format!("wrong data returned for key {key:#x}")),
            None => misses += 1,
        }
    }
    Ok((samples, misses))
}

/// Appends new workload items until `stop`; returns `(items, payload bytes)` written.
fn write_until(
    store: &mut dyn BlobStore,
    seed: u64,
    dist: SizeDist,
    stop: &AtomicBool,
) -> Result<(u64, u64)> {
    let mut w = Workload::new(seed, dist);
    let (mut items, mut bytes) = (0u64, 0u64);
    while !stop.load(Ordering::Relaxed) {
        let key = w.next_key();
        store.put(key, 2, &workload::payload(key)).map_err(err)?;
        items += 1;
        bytes += workload::payload_len(key) as u64;
        if items.is_multiple_of(16) {
            store.flush().map_err(err)?;
        }
    }
    store.flush().map_err(err)?;
    Ok((items, bytes))
}

/// Appends workload items to a custom store and reports progress after each flush, so a
/// test can kill this process at an arbitrary point (fault injection).
fn append_forever(f: &Flags) -> Result<()> {
    let dir = PathBuf::from(f.required("dir")?);
    let (seed, dist) = (f.number("seed", 1)?, f.dist()?);
    let limit = f.number("limit", u64::MAX)?;
    let mut store = CustomStore::open_writer(&dir).map_err(err)?;
    let mut w = Workload::new(seed, dist);
    let mut stdout = std::io::stdout().lock();
    for n in 1..=limit {
        let key = w.next_key();
        store.put(key, 3, &workload::payload(key)).map_err(err)?;
        if n % 8 == 0 {
            store.flush().map_err(err)?;
            writeln!(stdout, "flushed {n}")
                .and_then(|()| stdout.flush())
                .map_err(|e| e.to_string())?;
        }
    }
    store.flush().map_err(err)
}
