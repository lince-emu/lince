// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `lince run` end to end on the B1 programs in `tests/homebrew/` (the B1 exit criterion).

use std::process::{Command, Output};

fn homebrew(name: &str) -> String {
    format!("{}/../../tests/homebrew/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn lince(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lince"))
        .args(args)
        .output()
        .expect("start lince")
}

#[test]
fn run_prints_what_hello_writes() {
    let output = lince(&["run", &homebrew("hello.nro")]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello");
}

#[test]
fn run_prints_ok_for_every_self_checking_program() {
    for name in ["alu.nro", "branch.nro", "memory.nro"] {
        let mut out = Vec::new();
        let file = std::fs::read(homebrew(name)).expect("read the NRO");
        lince_app::run_nro(&file, &mut out).expect(name);
        assert_eq!(out, b"ok", "{name}");
    }
}

#[test]
fn a_file_that_is_not_an_nro_fails_without_output() {
    let mut out = Vec::new();
    let error = lince_app::run_nro(b"not an nro", &mut out).expect_err("load must fail");
    assert!(matches!(error, lince_app::AppError::Loader(_)), "{error:?}");
    assert_eq!(out, b"");
}

#[test]
fn a_missing_file_is_reported_with_a_failure_status() {
    let output = lince(&["run", &homebrew("missing.nro")]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("lince: cannot read"));
}

#[test]
fn wrong_arguments_print_the_usage() {
    for args in [&[][..], &["run"], &["start", "x.nro"]] {
        let output = lince(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("usage: lince run"));
    }
}
