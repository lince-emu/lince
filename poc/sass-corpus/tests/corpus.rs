// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The stored corpus (`out/`) must match a fresh build of `programs/`, and the T-023 runner
//! must get as far as the step that needs an NVIDIA GPU.
#![cfg(test)]

use std::process::Command;

fn corpus(subcommand: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_corpus"))
        .arg(subcommand)
        .output()
        .unwrap()
}

#[test]
fn stored_binaries_and_expected_outputs_match_a_fresh_build() {
    let out = corpus("check");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gpu_runner_stops_where_it_needs_a_gpu() {
    let out = corpus("run-gpu");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stdout).contains("corpus verified"));
}
