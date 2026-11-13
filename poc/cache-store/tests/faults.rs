// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fault injection for the custom store (PoC-6 correctness criteria): truncation, bit flips,
//! a writer killed mid-append, and a concurrent reader process. In every case a lookup must
//! return the exact payload or a miss — never wrong data.
#![cfg(test)]

mod support;

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use poc_cache_store::custom::CustomStore;
use poc_cache_store::workload::{SizeDist, SplitMix64};
use poc_cache_store::{BlobStore, workload};
use support::{fill, fresh_dir, hits_never_wrong, keys};

const DIST: SizeDist = SizeDist::Uniform {
    min: 1024,
    max: 8192,
};

fn only_file_matching(dir: &Path, prefix: &str) -> PathBuf {
    let mut matches: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix))
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected one {prefix}* in {}",
        dir.display()
    );
    matches.pop().unwrap()
}

/// How many of the records written for `keys`, in order, end at or before byte `cut` of a
/// single segment: 48-byte header plus payload, each record starting 16-byte aligned
/// (docs/02-cache-architecture.md §4.1).
fn records_ending_by(keys: &[u128], cut: u64) -> usize {
    let mut start = 0u64;
    keys.iter()
        .take_while(|&&k| {
            let end = start + 48 + workload::payload_len(k) as u64;
            start = end.div_ceil(16) * 16;
            end <= cut
        })
        .count()
}

fn flip_bit(path: &Path, bit: u64) {
    let mut bytes = fs::read(path).unwrap();
    let byte = usize::try_from(bit / 8).unwrap();
    bytes[byte] ^= 1 << (bit % 8);
    fs::write(path, bytes).unwrap();
}

#[test]
fn truncated_segment_gives_misses_not_wrong_data() {
    let ks = keys(41, DIST, 40);
    let mut rng = SplitMix64::new(41);
    for round in 0..6 {
        let dir = fresh_dir(&format!("truncate-{round}"));
        let mut store = CustomStore::open_writer(&dir).unwrap();
        fill(&mut store, &ks);
        drop(store);
        let seg = only_file_matching(&dir.join("seg"), "");
        let len = fs::metadata(&seg).unwrap().len();
        let cut = 1 + rng.below(len - 1);
        OpenOptions::new()
            .write(true)
            .open(&seg)
            .unwrap()
            .set_len(cut)
            .unwrap();

        let mut reader = CustomStore::open_read_only(&dir).unwrap();
        let hits = hits_never_wrong(&mut reader, &ks);
        let intact = records_ending_by(&ks, cut);
        assert_eq!(hits, intact, "cut at {cut} of {len}");
        // Every lost key was still indexed, so the record validation path rejected it.
        assert_eq!(reader.corrupt_misses(), (ks.len() - intact) as u64);
        drop(reader);
        fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn bit_flips_in_segments_and_index_never_return_wrong_data() {
    let dir = fresh_dir("bitflip");
    let ks = keys(43, DIST, 60);
    let mut store = CustomStore::open_writer(&dir).unwrap();
    fill(&mut store, &ks);
    drop(store);
    let seg = only_file_matching(&dir.join("seg"), "");
    let index = only_file_matching(&dir, "index.");
    let mut rng = SplitMix64::new(43);
    for target in [&seg, &index, &seg, &index] {
        let bits = fs::metadata(target).unwrap().len() * 8;
        for _ in 0..50 {
            let bit = rng.below(bits);
            flip_bit(target, bit);
            let mut reader = CustomStore::open_read_only(&dir).unwrap();
            hits_never_wrong(&mut reader, &ks);
            drop(reader);
            flip_bit(target, bit);
        }
    }
    let mut reader = CustomStore::open_read_only(&dir).unwrap();
    assert_eq!(
        hits_never_wrong(&mut reader, &ks),
        ks.len(),
        "flips were not undone"
    );
    drop(reader);
    fs::remove_dir_all(&dir).unwrap();
}

/// Starts `harness append-forever` on `dir` with small blobs so the index grows quickly.
fn spawn_appender(dir: &Path) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["append-forever", "--dir"])
        .arg(dir)
        .args(["--seed", "47", "--dist", "uniform:64:512"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

#[test]
fn concurrent_reader_process_then_writer_killed_mid_append() {
    let dir = fresh_dir("kill");
    let ks = keys(47, SizeDist::Uniform { min: 64, max: 512 }, 4000);
    let mut child = spawn_appender(&dir);
    let lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut reader: Option<CustomStore> = None;
    let mut flushed = 0usize;
    for line in lines {
        let line = line.unwrap();
        flushed = line.strip_prefix("flushed ").unwrap().parse().unwrap();
        // Everything the writer reported as flushed is visible to this other process.
        let r = reader.get_or_insert_with(|| CustomStore::open_read_only(&dir).unwrap());
        let start = flushed.saturating_sub(8);
        assert_eq!(hits_never_wrong(r, &ks[start..flushed]), flushed - start);
        if flushed >= 1600 {
            break;
        }
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let mut reader = reader.unwrap();
    assert!(
        reader.generation() > 1,
        "the reader followed at least one index growth"
    );
    assert_eq!(hits_never_wrong(&mut reader, &ks[..flushed]), flushed);
    drop(reader);

    // Recovery: reopen as the writer, append after the (possibly torn) tail, read everything.
    let mut writer = CustomStore::open_writer(&dir).unwrap();
    let extra = keys(53, DIST, 20);
    fill(&mut writer, &extra);
    assert_eq!(hits_never_wrong(&mut writer, &extra), extra.len());
    let survivors = hits_never_wrong(&mut writer, &ks);
    assert!(
        survivors >= flushed,
        "flushed records must survive the kill"
    );
    assert!(workload::verify(
        extra[0],
        &writer.get(extra[0]).unwrap().unwrap()
    ));
    drop(writer);
    fs::remove_dir_all(&dir).unwrap();
}
