// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `lince run` end to end on the B1 programs in `tests/homebrew/` (the B1 exit criterion).

use std::process::{Command, Output};

use lince_app::{Options, Report};
use lince_trace::Unimplemented;

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
        lince_app::run_nro(&file, &mut out, &Options::default()).expect(name);
        assert_eq!(out, b"ok", "{name}");
    }
}

#[test]
fn a_file_that_is_not_an_nro_fails_without_output() {
    let mut out = Vec::new();
    let error = lince_app::run_nro(b"not an nro", &mut out, &Options::default())
        .expect_err("load must fail");
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
    for args in [
        &[][..],
        &["run"],
        &["start", "x.nro"],
        &["run", "--sd"],
        &["run", "--sd", "dir"],
        &["run", "x.nro", "y.nro"],
        &["run", "--coverage", "--coverage", "x.nro"],
        &["run", "--verbose", "x.nro"],
    ] {
        let output = lince(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("usage: lince run"));
    }
}

#[test]
fn coverage_of_a_clean_run_is_an_empty_list() {
    let output = lince(&["run", "--coverage", &homebrew("hello.nro")]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "coverage: 0 unimplemented calls\n"
    );
}

#[test]
fn the_coverage_list_names_each_call_and_warns_about_dropped_events() {
    let report = Report {
        unimplemented: vec![
            Unimplemented::Svc(0x7F),
            Unimplemented::Command {
                object: "set",
                command: 5,
            },
        ],
        dropped: 2,
    };
    let mut out = Vec::new();
    lince_app::write_coverage(&mut out, &report).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out),
        concat!(
            "coverage: 2 unimplemented calls\n",
            "  SVC 0x7f\n",
            "  set command 5\n",
            "coverage: 2 once-only trace records dropped; the list may be incomplete\n",
        )
    );
}

/// The B2 exit criterion (doc 07): threads on several cores, synchronization and IPC to the
/// first services. Each line of `b2.expected` is one `OutputDebugString` call, which prints no
/// newline of its own, so the output is the lines run together.
#[test]
fn run_prints_the_expected_output_of_the_b2_program() {
    let output = lince(&["run", &homebrew("b2.nro")]);
    assert!(output.status.success(), "{output:?}");
    let expected = std::fs::read_to_string(homebrew("b2.expected")).expect("read b2.expected");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected.lines().collect::<String>()
    );
}

#[test]
fn coverage_of_the_b2_program_lists_nothing_and_warns_about_nothing() {
    let output = lince(&["run", "--coverage", &homebrew("b2.nro")]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "coverage: 0 unimplemented calls\n",
        "the scheduler's ledger records may overflow, but they hold no unknown calls"
    );
}

/// T-064: with `--sd`, `fsp-srv` is the host-directory file system, and the B2 program's
/// `fsp-srv` step (`SetCurrentProcess`, `OpenSdCardFileSystem`, `CloseHandle`) still passes.
#[test]
fn the_b2_program_passes_with_a_host_sd_card() {
    let sd = std::env::temp_dir().join(format!("lince-run-sd-{}", std::process::id()));
    std::fs::create_dir_all(&sd).expect("create the SD directory");
    let output = lince(&["run", "--sd", &sd.to_string_lossy(), &homebrew("b2.nro")]);
    std::fs::remove_dir_all(&sd).expect("remove the SD directory");
    assert!(output.status.success(), "{output:?}");
    let expected = std::fs::read_to_string(homebrew("b2.expected")).expect("read b2.expected");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected.lines().collect::<String>()
    );
}

#[test]
fn a_missing_sd_directory_is_reported_before_the_run() {
    let missing = std::env::temp_dir().join("lince-run-sd-missing-directory");
    let output = lince(&[
        "run",
        "--sd",
        &missing.to_string_lossy(),
        &homebrew("hello.nro"),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("as the SD card"),
        "{output:?}"
    );
}
