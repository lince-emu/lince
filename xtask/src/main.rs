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
mod commits;
mod cpp;
mod error;
mod license;
mod process;
mod repository;
mod spdx;

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo xtask <command>

commands:
  ci [--base REV] [--pr-title TITLE]
        run every check that CI runs (fmt, commit messages, SPDX, license, invariants,
        C++ format/lint/sanitizers, clippy, tests, deny); commits are checked from REV (default origin/main) to HEAD";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(range) = parse_ci_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match ci::run(workspace_root(), &range) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Parses `ci [--base REV] [--pr-title TITLE]`; `None` for anything else.
fn parse_ci_args(args: &[String]) -> Option<commits::Range> {
    let (command, mut rest) = args.split_first()?;
    if command != "ci" {
        return None;
    }
    let mut range = commits::Range::default();
    while let [flag, value, tail @ ..] = rest {
        match flag.as_str() {
            "--base" => range.base = Some(value.clone()),
            "--pr-title" => range.pr_title = Some(value.clone()),
            _ => return None,
        }
        rest = tail;
    }
    rest.is_empty().then_some(range)
}

/// The workspace root: `xtask/` is one of its direct children.
fn workspace_root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
}

#[cfg(test)]
mod tests {
    use super::parse_ci_args;
    use crate::commits::Range;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn ci_accepts_base_and_pr_title_in_any_order() {
        let range = parse_ci_args(&args(&[
            "ci",
            "--pr-title",
            "feat(x): y",
            "--base",
            "origin/main",
        ]));
        assert_eq!(
            range,
            Some(Range {
                base: Some("origin/main".to_owned()),
                pr_title: Some("feat(x): y".to_owned()),
            })
        );
        assert_eq!(parse_ci_args(&args(&["ci"])), Some(Range::default()));
    }

    #[test]
    fn unknown_commands_flags_and_missing_values_are_rejected() {
        assert_eq!(parse_ci_args(&args(&["bench"])), None);
        assert_eq!(parse_ci_args(&args(&["ci", "--verbose", "1"])), None);
        assert_eq!(parse_ci_args(&args(&["ci", "--base"])), None);
        assert_eq!(parse_ci_args(&args(&[])), None);
    }
}
