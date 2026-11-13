// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Corpus tool.
//!
//! - `corpus build`: assembles every `programs/*.sass` and writes `out/<name>.bin` and
//!   `out/<name>.expected`.
//! - `corpus check`: fails if a stored artifact differs from a fresh build.
//! - `corpus run-gpu`: the T-023 runner; checks the stored corpus, then stops where it needs an
//!   NVIDIA GPU.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line tool: results go to the terminal"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use poc_sass_corpus::corpus::{self, Artifacts};

type Result<T> = std::result::Result<T, String>;

/// Exit code of `run-gpu` when it stops at the step that needs an NVIDIA GPU.
const NEEDS_GPU: u8 = 2;

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let outcome = match std::env::args().nth(1).as_deref() {
        Some("build") => build(&root),
        Some("check") => check(&root),
        Some("run-gpu") => return run_gpu(&root),
        _ => Err("usage: corpus build | check | run-gpu".to_owned()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

/// Every program as `(name, artifacts)`, sorted by name.
fn build_all(root: &Path) -> Result<Vec<(String, Artifacts)>> {
    let dir = root.join("programs");
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("read {}: {e}", dir.display()))?
        .map(|e| e.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<_>>()?;
    entries.retain(|p| p.extension().is_some_and(|x| x == "sass"));
    entries.sort();
    entries
        .iter()
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| format!("bad file name {}", path.display()))?
                .to_owned();
            let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
            let artifacts = corpus::build(&source).map_err(|e| format!("{name}: {e}"))?;
            Ok((name, artifacts))
        })
        .collect()
}

fn build(root: &Path) -> Result<()> {
    let out = root.join("out");
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for (name, a) in build_all(root)? {
        fs::write(out.join(format!("{name}.bin")), &a.binary).map_err(|e| e.to_string())?;
        fs::write(out.join(format!("{name}.expected")), &a.expected).map_err(|e| e.to_string())?;
        println!("{name}: {} instructions", a.binary.len() / 16);
    }
    Ok(())
}

fn check(root: &Path) -> Result<()> {
    let out = root.join("out");
    let mut stale = Vec::new();
    for (name, a) in build_all(root)? {
        let bin = fs::read(out.join(format!("{name}.bin"))).unwrap_or_default();
        let expected = fs::read_to_string(out.join(format!("{name}.expected"))).unwrap_or_default();
        if bin != a.binary || expected != a.expected {
            stale.push(name);
        }
    }
    if stale.is_empty() {
        println!("corpus is up to date");
        Ok(())
    } else {
        Err(format!(
            "stale or missing: {}; run `corpus build`",
            stale.join(", ")
        ))
    }
}

/// The T-023 runner. Loading the programs on a GPU needs an NVIDIA driver and a code-object
/// container for the binaries, which T-023 adds; this step verifies everything before that.
fn run_gpu(root: &Path) -> ExitCode {
    if let Err(msg) = check(root) {
        eprintln!("{msg}");
        return ExitCode::FAILURE;
    }
    let driver = driver_library();
    println!(
        "corpus verified; next step: load each out/*.bin on an NVIDIA GPU of compute capability \
         8.6–8.9 through the driver (no SDK tool, LEGAL-15) and compare its memory with \
         out/*.expected under the doc 06 matching rule"
    );
    match driver {
        Some(path) => eprintln!(
            "NVIDIA driver found at {}; launching is not implemented yet (T-023)",
            path.display()
        ),
        None => eprintln!("no NVIDIA driver found: this step needs an NVIDIA GPU (T-023)"),
    }
    ExitCode::from(NEEDS_GPU)
}

/// The NVIDIA driver's user-mode library, if this host has one.
fn driver_library() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &["C:\\Windows\\System32\\nvcuda.dll"]
    } else {
        &[
            "/usr/lib/x86_64-linux-gnu/libcuda.so.1",
            "/usr/lib64/libcuda.so.1",
            "/usr/lib/libcuda.so.1",
        ]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
}
