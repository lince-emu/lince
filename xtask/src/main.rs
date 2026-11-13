// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lince build automation (ADR-0002).
//!
//! # Role
//! One entry point for the checks that CI and developers run identically on Windows and
//! Linux: `cargo xtask ci`. Uses only the standard library.
//!
//! # References
//! docs/04-rust-architecture.md §7 · ADR-0002 · docs/conventions/lints.md
#![forbid(unsafe_code)]
#![expect(
    clippy::print_stderr,
    reason = "xtask is a developer tool that reports progress on the terminal (QUAL-43)"
)]

mod ci;
mod error;
mod license;
mod process;
mod repository;
mod spdx;

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>

commands:
  ci    run every check that CI runs (fmt, SPDX, license, invariants, clippy, tests, deny)";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() != Some("ci") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match ci::run(workspace_root()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The workspace root: `xtask/` is one of its direct children.
fn workspace_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}
