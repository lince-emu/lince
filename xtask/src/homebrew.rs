// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask homebrew`: rebuilds the B1 test programs (T-029).
//!
//! Every `tests/homebrew/<name>.s` except the shared `start.s` is assembled with
//! `clang --target=aarch64-none-elf`, linked after `start.s` with `ld.lld` and
//! `tests/homebrew/nro.ld`, and converted by `elf2nro` to `tests/homebrew/<name>.nro`. The NROs
//! are checked in so tests need no LLVM; `--check` rebuilds them and fails if any differs.
//! `cargo xtask ci` only builds them ([`Mode::Build`]): LLVM versions differ between hosts, so
//! the bytes may too, but every program must still assemble and link.

use std::fs;
use std::path::{Path, PathBuf};

use crate::cpp;
use crate::error::XtaskError;
use crate::process;

const SOURCE_DIR: &str = "tests/homebrew";
const START: &str = "start";
const LINKER_SCRIPT: &str = "tests/homebrew/nro.ld";
/// Objects and ELF files, under the workspace root.
const BUILD_DIR: &str = "target/xtask-homebrew";
const TARGET: &str = "--target=aarch64-none-elf";

/// What to do with each freshly built NRO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Overwrite the checked-in NRO.
    Write,
    /// Fail if the checked-in NRO differs.
    Check,
    /// Only build; leave the checked-in NRO alone.
    Build,
}

/// Rebuilds every test NRO and handles the result as `mode` says.
pub(crate) fn run(root: &Path, mode: Mode) -> Result<(), XtaskError> {
    let clang = cpp::tool(root, "clang")?;
    let lld = cpp::tool(root, "ld.lld")?;
    let build = root.join(BUILD_DIR);
    fs::create_dir_all(&build).map_err(|source| XtaskError::Write {
        path: build.clone(),
        source,
    })?;
    let start = assemble(root, &clang, START)?;
    let mut differ = 0;
    for name in programs(root)? {
        let object = assemble(root, &clang, &name)?;
        let elf = build.join(format!("{name}.elf"));
        let link = [
            "-T",
            LINKER_SCRIPT,
            "--build-id=none",
            "-nostdlib",
            "-static",
            "-o",
            path_str(&elf)?,
            path_str(&start)?,
            path_str(&object)?,
        ];
        process::run_step("ld.lld", &mut process::command(root, &lld, &link))?;
        let nro = elf2nro::convert(&read(&elf)?).map_err(|source| XtaskError::Convert {
            path: elf.clone(),
            source,
        })?;
        let output = root.join(SOURCE_DIR).join(format!("{name}.nro"));
        match mode {
            Mode::Write => fs::write(&output, nro).map_err(|source| XtaskError::Write {
                path: output,
                source,
            })?,
            Mode::Check => {
                if fs::read(&output).ok().as_deref() != Some(nro.as_slice()) {
                    eprintln!("  {}: differs from a fresh build", output.display());
                    differ += 1;
                }
            }
            Mode::Build => {}
        }
    }
    if differ == 0 {
        Ok(())
    } else {
        Err(XtaskError::Violations {
            rule: "T-029 reproducible NRO",
            count: differ,
        })
    }
}

/// Program names: the `.s` files of the source directory except the start block, sorted.
fn programs(root: &Path) -> Result<Vec<String>, XtaskError> {
    let dir = root.join(SOURCE_DIR);
    let entries = fs::read_dir(&dir).map_err(|source| XtaskError::Read {
        path: dir.clone(),
        source,
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| XtaskError::Read {
                path: dir.clone(),
                source,
            })?
            .path();
        if path.extension().is_some_and(|extension| extension == "s")
            && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
            && stem != START
        {
            names.push(stem.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

fn assemble(root: &Path, clang: &Path, name: &str) -> Result<PathBuf, XtaskError> {
    let source = format!("{SOURCE_DIR}/{name}.s");
    let object = root.join(BUILD_DIR).join(format!("{name}.o"));
    let args = [
        TARGET,
        "-c",
        "-I",
        SOURCE_DIR,
        &source,
        "-o",
        path_str(&object)?,
    ];
    process::run_step("clang", &mut process::command(root, clang, &args))?;
    Ok(object)
}

fn read(path: &Path) -> Result<Vec<u8>, XtaskError> {
    fs::read(path).map_err(|source| XtaskError::Read {
        path: path.to_owned(),
        source,
    })
}

fn path_str(path: &Path) -> Result<&str, XtaskError> {
    path.to_str()
        .ok_or(XtaskError::NonUtf8Output { program: "path" })
}
