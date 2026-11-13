// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A reader in another process (doc 02 §5, §10; D-002): `harness resolve --reader process`
//! must time a child-process reader for every engine while this process's writer appends,
//! and an engine that cannot open the store from a second process must be reported, not crash
//! the run.
#![cfg(test)]

mod support;

use std::path::Path;
use std::process::Command;

use support::fresh_dir;

/// Runs `harness resolve --writer on --reader process` for `engine` and returns its CSV row.
fn resolve_with_process_reader(engine: &str, dir: &Path) -> Vec<String> {
    let out_csv = dir.join("resolve.csv");
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["resolve", "--engine", engine, "--dir"])
        .arg(dir)
        .args(["--size", "1M", "--lookups", "200", "--writer", "on"])
        .args([
            "--reader",
            "process",
            "--dist",
            "uniform:1024:8192",
            "--out",
        ])
        .arg(&out_csv)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "harness failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let csv = std::fs::read_to_string(&out_csv).unwrap();
    let mut lines = csv.lines();
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    let row: Vec<&str> = lines.next().unwrap().split(',').collect();
    assert_eq!(header.len(), row.len());
    header
        .iter()
        .zip(row)
        .map(|(h, v)| format!("{h}={v}"))
        .collect()
}

fn assert_served_without_misses(engine: &str) {
    let dir = fresh_dir(&format!("second-process-{engine}"));
    let row = resolve_with_process_reader(engine, &dir);
    for expected in ["reader=process", "served=yes", "misses=0", "writer=on"] {
        assert!(
            row.iter().any(|f| f == expected),
            "{engine}: missing {expected} in {row:?}"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn custom_store_serves_a_reader_in_another_process() {
    assert_served_without_misses("custom");
}

#[test]
fn redb_in_single_writer_mode_serves_a_reader_in_another_process() {
    assert_served_without_misses("redb");
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_in_wal_mode_serves_a_reader_in_another_process() {
    assert_served_without_misses("sqlite");
}

#[test]
fn a_store_the_reader_cannot_open_is_reported_as_unserved() {
    let dir = fresh_dir("second-process-missing");
    let out = Command::new(env!("CARGO_BIN_EXE_harness"))
        .args(["resolve-reader", "--engine", "custom", "--dir"])
        .arg(dir.join("no-store-here"))
        .args(["--items", "1", "--lookups", "1"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("unserved "), "got `{stdout}`");
    std::fs::remove_dir_all(&dir).unwrap();
}
