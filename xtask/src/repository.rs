// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Listing and reading the repository's files.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::error::XtaskError;
use crate::process;

/// Tracked and untracked-but-not-ignored files, as `/`-separated paths relative to `root`.
pub(crate) fn files(root: &Path) -> Result<Vec<String>, XtaskError> {
    let listing = process::capture(
        root,
        "git",
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    Ok(listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Reads a repository file as text; `None` when it is tracked but deleted in the work tree.
pub(crate) fn read(root: &Path, file: &str) -> Result<Option<String>, XtaskError> {
    let path = root.join(file);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(XtaskError::Read { path, source }),
    }
}
