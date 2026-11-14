// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lince build automation (ADR-0002).
//!
//! # Role
//! One entry point for the checks that CI and developers run identically on Windows and
//! Linux (`cargo xtask ci`) and for rebuilding generated test inputs (`cargo xtask homebrew`).
//! Uses only the standard library and the workspace's own tools.
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
mod homebrew;
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
        C++ format/lint/sanitizers, clippy, tests, deny); commits are checked from REV (default origin/main) to HEAD
  homebrew [--check]
        rebuild tests/homebrew/*.nro with LLVM; --check fails if a rebuild differs
  pr-title TITLE
        check a pull-request title with the rule `ci --pr-title` applies";

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Ci(commits::Range),
    Homebrew { check: bool },
    PrTitle(String),
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let result = match command {
        Command::Ci(range) => ci::run(workspace_root(), &range),
        Command::Homebrew { check } => homebrew::run(workspace_root(), check),
        Command::PrTitle(title) => {
            return match commits::validate_pr_title(&title) {
                Ok(()) => {
                    eprintln!("xtask: pull-request title is valid");
                    ExitCode::SUCCESS
                }
                Err(reason) => {
                    eprintln!("xtask: pull-request title {title:?}: {reason}");
                    ExitCode::FAILURE
                }
            };
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Parses the command line; `None` for anything [`USAGE`] does not describe.
fn parse_args(args: &[String]) -> Option<Command> {
    match args {
        [command, rest @ ..] if command == "ci" => parse_ci_args(rest).map(Command::Ci),
        [command] if command == "homebrew" => Some(Command::Homebrew { check: false }),
        [command, flag] if command == "homebrew" && flag == "--check" => {
            Some(Command::Homebrew { check: true })
        }
        [command, title] if command == "pr-title" => Some(Command::PrTitle(title.clone())),
        _ => None,
    }
}

/// Parses the arguments after `ci`: `[--base REV] [--pr-title TITLE]`.
fn parse_ci_args(mut rest: &[String]) -> Option<commits::Range> {
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
    use super::{Command, parse_args};
    use crate::commits::Range;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn ci_accepts_base_and_pr_title_in_any_order() {
        let command = parse_args(&args(&[
            "ci",
            "--pr-title",
            "feat(x): y",
            "--base",
            "origin/main",
        ]));
        assert_eq!(
            command,
            Some(Command::Ci(Range {
                base: Some("origin/main".to_owned()),
                pr_title: Some("feat(x): y".to_owned()),
            }))
        );
        assert_eq!(
            parse_args(&args(&["ci"])),
            Some(Command::Ci(Range::default()))
        );
    }

    #[test]
    fn homebrew_takes_an_optional_check_flag() {
        assert_eq!(
            parse_args(&args(&["homebrew"])),
            Some(Command::Homebrew { check: false })
        );
        assert_eq!(
            parse_args(&args(&["homebrew", "--check"])),
            Some(Command::Homebrew { check: true })
        );
        assert_eq!(
            parse_args(&args(&["pr-title", "feat(x): y"])),
            Some(Command::PrTitle("feat(x): y".to_owned()))
        );
    }

    #[test]
    fn unknown_commands_flags_and_missing_values_are_rejected() {
        assert_eq!(parse_args(&args(&["bench"])), None);
        assert_eq!(parse_args(&args(&["ci", "--verbose", "1"])), None);
        assert_eq!(parse_args(&args(&["ci", "--base"])), None);
        assert_eq!(parse_args(&args(&["homebrew", "--write"])), None);
        assert_eq!(parse_args(&args(&["pr-title"])), None);
        assert_eq!(parse_args(&args(&["pr-title", "a", "b"])), None);
        assert_eq!(parse_args(&args(&[])), None);
    }
}
