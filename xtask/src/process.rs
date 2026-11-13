// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Running external programs (cargo, git, bash) from the workspace root.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::XtaskError;

/// A command for `program` with `args`, run from the workspace root.
pub(crate) fn command(root: &Path, program: impl AsRef<Path>, args: &[&str]) -> Command {
    let mut command = Command::new(program.as_ref());
    command.args(args).current_dir(root);
    command
}

/// Runs one check step with inherited output and fails if the program fails.
pub(crate) fn run_step(step: &'static str, command: &mut Command) -> Result<(), XtaskError> {
    eprintln!("xtask ci: {step}");
    let status = command
        .status()
        .map_err(|source| spawn_error(command, source))?;
    if status.success() {
        Ok(())
    } else {
        Err(XtaskError::StepFailed { step, status })
    }
}

/// Runs a program silently and reports whether it exited successfully.
pub(crate) fn succeeds(command: &mut Command) -> Result<bool, XtaskError> {
    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|source| spawn_error(command, source))?;
    Ok(status.success())
}

/// Runs a program and returns its standard output.
pub(crate) fn capture(
    root: &Path,
    program: &'static str,
    args: &[&str],
) -> Result<String, XtaskError> {
    let mut command = command(root, program, args);
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .map_err(|source| spawn_error(&command, source))?;
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
    let exec_path = PathBuf::from(capture(root, "git", &["--exec-path"])?.trim());
    git_install_root(&exec_path)
        .map(|git_root| git_root.join("bin").join("bash.exe"))
        .ok_or(XtaskError::GitBashNotFound { exec_path })
}

/// `git --exec-path` on Git for Windows is `<git>/<mingw64|ucrt64>/libexec/git-core`.
fn git_install_root(exec_path: &Path) -> Option<&Path> {
    let root = exec_path.ancestors().nth(3)?;
    (root.parent().is_some() || root.has_root()).then_some(root)
}

fn spawn_error(command: &Command, source: std::io::Error) -> XtaskError {
    XtaskError::Spawn {
        program: PathBuf::from(command.get_program()),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_install_root_is_found_for_a_mingw64_layout() {
        let exec_path = Path::new("C:/Program Files/Git/mingw64/libexec/git-core");
        assert_eq!(
            git_install_root(exec_path),
            Some(Path::new("C:/Program Files/Git"))
        );
    }

    #[test]
    fn git_install_root_is_found_for_a_ucrt64_layout() {
        let exec_path = Path::new("C:/Program Files/Git/ucrt64/libexec/git-core");
        assert_eq!(
            git_install_root(exec_path),
            Some(Path::new("C:/Program Files/Git"))
        );
    }

    #[test]
    fn git_install_root_is_none_when_the_path_is_too_short() {
        assert_eq!(git_install_root(Path::new("libexec/git-core")), None);
    }
}
