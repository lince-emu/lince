// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Running external programs (cargo, git, bash) from the workspace root.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::XtaskError;

/// Runs one check step with inherited output and fails if the program fails.
pub(crate) fn run_step(
    root: &Path,
    step: &'static str,
    program: &Path,
    args: &[&str],
) -> Result<(), XtaskError> {
    eprintln!("xtask ci: {step}");
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|source| spawn_error(program, source))?;
    if status.success() {
        Ok(())
    } else {
        Err(XtaskError::StepFailed { step, status })
    }
}

/// Runs a program silently and reports whether it exited successfully.
pub(crate) fn succeeds(root: &Path, program: &Path, args: &[&str]) -> Result<bool, XtaskError> {
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|source| spawn_error(program, source))?;
    Ok(status.success())
}

/// Runs a program and returns its standard output.
pub(crate) fn capture(
    root: &Path,
    program: &'static str,
    args: &[&str],
) -> Result<String, XtaskError> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|source| spawn_error(Path::new(program), source))?;
    if !output.status.success() {
        return Err(XtaskError::StepFailed {
            step: program,
            status: output.status,
        });
    }
    String::from_utf8(output.stdout).map_err(|_| XtaskError::NonUtf8Output { program })
}

/// The bash that runs the repository's shell scripts.
///
/// On Windows, `Command::new("bash")` searches the system directory before `PATH` and would
/// start WSL's `bash.exe`; the hooks are written for the Git for Windows bash instead.
pub(crate) fn bash(root: &Path) -> Result<PathBuf, XtaskError> {
    if !cfg!(windows) {
        return Ok(PathBuf::from("bash"));
    }
    // `git --exec-path` is `<git>/<mingw64|ucrt64>/libexec/git-core`; bash is `<git>/bin`.
    let exec_path = PathBuf::from(capture(root, "git", &["--exec-path"])?.trim());
    let git_root = exec_path
        .ancestors()
        .nth(3)
        .ok_or_else(|| XtaskError::GitBashNotFound {
            exec_path: exec_path.clone(),
        })?;
    Ok(git_root.join("bin").join("bash.exe"))
}

fn spawn_error(program: &Path, source: std::io::Error) -> XtaskError {
    XtaskError::Spawn {
        program: program.to_path_buf(),
        source,
    }
}
