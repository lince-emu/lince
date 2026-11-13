// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Errors reported by xtask commands (PAT-01 shape, without `thiserror` to keep xtask
//! dependency-free).

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;

/// Why an xtask command failed.
#[derive(Debug)]
pub(crate) enum XtaskError {
    /// A program could not be started.
    Spawn { program: PathBuf, source: io::Error },
    /// A step ran and reported failure; its own output explains why.
    StepFailed {
        step: &'static str,
        status: ExitStatus,
    },
    /// A required external tool is not installed.
    MissingTool {
        tool: &'static str,
        install: &'static str,
    },
    /// A program printed output that is not UTF-8.
    NonUtf8Output { program: &'static str },
    /// The Git for Windows installation could not be derived from `git --exec-path`.
    GitBashNotFound { exec_path: PathBuf },
    /// A repository file could not be read.
    Read { path: PathBuf, source: io::Error },
    /// A repository-wide check found violations, already listed on stderr.
    Violations { rule: &'static str, count: usize },
}

impl fmt::Display for XtaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { program, source } => {
                write!(f, "could not start `{}`: {source}", program.display())
            }
            Self::StepFailed { step, status } => write!(f, "{step} failed ({status})"),
            Self::MissingTool { tool, install } => {
                write!(f, "{tool} is not installed; install it with `{install}`")
            }
            Self::NonUtf8Output { program } => {
                write!(f, "`{program}` printed output that is not UTF-8")
            }
            Self::GitBashNotFound { exec_path } => write!(
                f,
                "cannot locate Git for Windows bash from git exec path {}",
                exec_path.display()
            ),
            Self::Read { path, source } => {
                write!(f, "could not read {}: {source}", path.display())
            }
            Self::Violations { rule, count } => {
                write!(f, "{count} {rule} violation(s), listed above")
            }
        }
    }
}

impl std::error::Error for XtaskError {}
