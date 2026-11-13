// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask ci`: every check CI runs, cheapest first, stopping at the first failure.

use std::path::Path;

use crate::error::XtaskError;
use crate::license;
use crate::process;
use crate::repository;
use crate::spdx;

const CARGO: &str = "cargo";
const INVARIANT_SCRIPT: &str = ".claude/hooks/check-invariants.sh";
/// Files passed to one run of the invariant script; keeps command lines short on Windows.
const INVARIANT_BATCH: usize = 200;

/// Runs fmt, SPDX, license, invariant, clippy, test and cargo-deny checks.
pub(crate) fn run(root: &Path) -> Result<(), XtaskError> {
    let cargo = Path::new(CARGO);
    process::run_step(root, "rustfmt", cargo, &["fmt", "--all", "--check"])?;
    let files = repository::files(root)?;
    spdx::check(root, &files)?;
    license::check(root, &files)?;
    invariants(root, &files)?;
    process::run_step(
        root,
        "clippy",
        cargo,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    process::run_step(root, "tests", cargo, &["test", "--workspace", "--locked"])?;
    cargo_deny(root)?;
    eprintln!("xtask ci: all checks passed");
    Ok(())
}

/// Runs the AGENTS.md §2 text checks over every file, not only the files changed on a branch.
fn invariants(root: &Path, files: &[String]) -> Result<(), XtaskError> {
    let bash = process::bash(root)?;
    for batch in files.chunks(INVARIANT_BATCH) {
        let mut args = vec![INVARIANT_SCRIPT, "--files"];
        args.extend(batch.iter().map(String::as_str));
        process::run_step(root, "invariant check", &bash, &args)?;
    }
    Ok(())
}

fn cargo_deny(root: &Path) -> Result<(), XtaskError> {
    let cargo = Path::new(CARGO);
    if !process::succeeds(root, cargo, &["deny", "--version"])? {
        return Err(XtaskError::MissingTool {
            tool: "cargo-deny",
            install: "cargo install cargo-deny --locked",
        });
    }
    process::run_step(root, "cargo deny", cargo, &["deny", "check"])
}
