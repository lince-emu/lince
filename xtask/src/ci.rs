// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask ci`: every check CI runs, cheapest first, stopping at the first failure.

use std::path::Path;

use crate::commits;
use crate::cpp;
use crate::error::XtaskError;
use crate::license;
use crate::process;
use crate::repository;
use crate::spdx;

const CARGO: &str = "cargo";
const INVARIANT_SCRIPT: &str = ".claude/hooks/check-invariants.sh";
/// The invariant script checks the tree named by this variable when it is set
/// (`.claude/hooks/lib.sh`); xtask always points it at the workspace being checked.
const PROJECT_DIR_VAR: &str = "CLAUDE_PROJECT_DIR";
/// Files passed to one run of the invariant script; keeps command lines short on Windows.
const INVARIANT_BATCH: usize = 200;

/// Runs fmt, commit-message, SPDX, license, invariant, C++, clippy, test and cargo-deny checks.
pub(crate) fn run(root: &Path, range: &commits::Range) -> Result<(), XtaskError> {
    process::run_step(
        "rustfmt",
        &mut process::command(root, CARGO, &["fmt", "--all", "--check"]),
    )?;
    commits::check(root, range)?;
    let files = repository::files(root)?;
    spdx::check(root, &files)?;
    license::check(root, &files)?;
    invariants(root, &files)?;
    cpp::check(root, &files)?;
    let clippy = [
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ];
    process::run_step("clippy", &mut process::command(root, CARGO, &clippy))?;
    process::run_step(
        "tests",
        &mut process::command(root, CARGO, &["test", "--workspace", "--locked"]),
    )?;
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
        let mut command = process::command(root, &bash, &args);
        command.env(PROJECT_DIR_VAR, root);
        process::run_step("invariant check", &mut command)?;
    }
    Ok(())
}

fn cargo_deny(root: &Path) -> Result<(), XtaskError> {
    if !process::succeeds(&mut process::command(root, CARGO, &["deny", "--version"]))? {
        return Err(XtaskError::MissingTool {
            tool: "cargo-deny",
            install: "cargo install cargo-deny --locked",
        });
    }
    process::run_step(
        "cargo deny",
        &mut process::command(root, CARGO, &["deny", "check"]),
    )
}
