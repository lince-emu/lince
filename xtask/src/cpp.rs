// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! C++ checks (ARCH-54, ADR-0030): `clang-format`, `clang-tidy` with warnings as errors, and
//! every `cpp/tests/*.cpp` driver built with the address and undefined-behavior sanitizers
//! and run.
//!
//! A component is a directory with a `cpp/` subdirectory (`crates/<crate>/cpp/` or
//! `poc/<slug>/cpp/`): its `cpp/src/*.cpp` are linked into each of its test drivers, and
//! `cpp/include/` is on the include path. LLVM tools are taken from `PATH`, then from the LLVM
//! installer's default directory on Windows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::XtaskError;
use crate::process;

const CPP_EXTENSIONS: [&str; 4] = ["cpp", "hpp", "h", "cc"];
/// Where the official LLVM installer puts its tools on Windows.
const WINDOWS_LLVM_BIN: &str = "C:\\Program Files\\LLVM\\bin";
/// Where sanitizer test binaries are written, under the workspace root.
const BUILD_DIR: &str = "target/xtask-cpp";
const CPP_STD: &str = "c++20";

/// One C++ component: its root (the parent of `cpp/`) and its files.
#[derive(Default)]
struct Component {
    sources: Vec<String>,
    tests: Vec<String>,
}

/// Runs every C++ check over the C and C++ files in `files`; does nothing when there are none.
pub(crate) fn check(root: &Path, files: &[String]) -> Result<(), XtaskError> {
    let cpp: Vec<&String> = files.iter().filter(|f| is_cpp(f)).collect();
    if cpp.is_empty() {
        return Ok(());
    }
    let format = tool(root, "clang-format")?;
    let mut args = vec!["--dry-run", "-Werror"];
    args.extend(cpp.iter().map(|f| f.as_str()));
    process::run_step("clang-format", &mut process::command(root, &format, &args))?;
    let components = components(&cpp);
    let tidy = tool(root, "clang-tidy")?;
    for (dir, component) in &components {
        tidy_component(root, &tidy, dir, component)?;
    }
    for (dir, component) in &components {
        sanitize_component(root, dir, component)?;
    }
    Ok(())
}

fn is_cpp(file: &str) -> bool {
    Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| CPP_EXTENSIONS.contains(&e))
}

/// Groups `cpp/src` and `cpp/tests` sources by the directory that holds `cpp/`.
fn components<'a>(files: &[&'a String]) -> BTreeMap<&'a str, Component> {
    let mut map: BTreeMap<&str, Component> = BTreeMap::new();
    for file in files {
        let Some((dir, inside)) = file.split_once("/cpp/") else {
            continue;
        };
        let entry = map.entry(dir).or_default();
        if !Path::new(file)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cpp"))
        {
            continue;
        }
        if inside.starts_with("src/") {
            entry.sources.push((*file).clone());
        } else if inside.starts_with("tests/") {
            entry.tests.push((*file).clone());
        }
    }
    map
}

/// An LLVM tool by name: from `PATH`, else from the Windows installer's default directory.
fn tool(root: &Path, name: &'static str) -> Result<PathBuf, XtaskError> {
    // A program missing from `PATH` fails to start; that is "not found", not an error.
    let on_path = process::command(root, name, &["--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if on_path {
        return Ok(PathBuf::from(name));
    }
    let installed = Path::new(WINDOWS_LLVM_BIN).join(format!("{name}.exe"));
    if cfg!(windows) && installed.exists() {
        return Ok(installed);
    }
    Err(XtaskError::MissingTool {
        tool: name,
        install: "install LLVM (winget install LLVM.LLVM, or apt install clang clang-tidy clang-format)",
    })
}

fn tidy_component(
    root: &Path,
    tidy: &Path,
    dir: &str,
    component: &Component,
) -> Result<(), XtaskError> {
    let include = format!("-I{dir}/cpp/include");
    let std = format!("-std={CPP_STD}");
    for file in component.sources.iter().chain(&component.tests) {
        let args = [
            "--quiet",
            file.as_str(),
            "--",
            std.as_str(),
            "-fno-exceptions",
            "-fno-rtti",
            include.as_str(),
        ];
        process::run_step("clang-tidy", &mut process::command(root, tidy, &args))?;
    }
    Ok(())
}

/// Builds each test driver with the address and undefined-behavior sanitizers and runs it.
fn sanitize_component(root: &Path, dir: &str, component: &Component) -> Result<(), XtaskError> {
    let out_dir = root.join(BUILD_DIR).join(dir.replace('/', "_"));
    std::fs::create_dir_all(&out_dir).map_err(|source| XtaskError::Read {
        path: out_dir.clone(),
        source,
    })?;
    for test in &component.tests {
        let stem = Path::new(test)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("test");
        let exe = out_dir.join(format!("{stem}{}", std::env::consts::EXE_SUFFIX));
        let mut build = sanitized_build(root, dir, &exe)?;
        build.args(&component.sources).arg(test);
        process::run_step("sanitizer build", &mut build)?;
        let mut run = Command::new(&exe);
        run.current_dir(root);
        if cfg!(windows) {
            add_runtime_dir(root, &mut run)?;
        }
        process::run_step("sanitizer run", &mut run)?;
    }
    Ok(())
}

/// The compiler command with sanitizer, standard and no-exception/no-RTTI flags (ARCH-51).
fn sanitized_build(root: &Path, dir: &str, exe: &Path) -> Result<Command, XtaskError> {
    let include = format!("{dir}/cpp/include");
    let sanitizers = [
        "-fsanitize=address",
        "-fsanitize=undefined",
        "-fno-sanitize-recover=all",
    ];
    if cfg!(windows) {
        let mut c = process::command(root, tool(root, "clang-cl")?, &[]);
        c.args([
            "/nologo",
            "/std:c++20",
            "/GR-",
            "/EHs-c-",
            "/W4",
            "/WX",
            "/Zi",
            "/MD",
        ])
        .args(sanitizers)
        .arg(format!("/I{include}"))
        .arg(format!("/Fe{}", exe.display()))
        .arg(format!("/Fo{}\\", exe.parent().unwrap_or(root).display()));
        Ok(c)
    } else {
        let mut c = process::command(root, tool(root, "clang++")?, &[]);
        c.arg(format!("-std={CPP_STD}"))
            .args([
                "-fno-exceptions",
                "-fno-rtti",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-g",
            ])
            .args(sanitizers)
            .arg(format!("-I{include}"))
            .arg("-o")
            .arg(exe);
        Ok(c)
    }
}

/// On Windows the sanitizer runtime is a DLL in clang's resource directory; put it on `PATH`.
fn add_runtime_dir(root: &Path, run: &mut Command) -> Result<(), XtaskError> {
    let clang = tool(root, "clang")?;
    let output = process::command(root, &clang, &["-print-resource-dir"])
        .output()
        .map_err(|source| XtaskError::Spawn {
            program: clang.clone(),
            source,
        })?;
    let resource = String::from_utf8(output.stdout)
        .map_err(|_| XtaskError::NonUtf8Output { program: "clang" })?;
    let runtime = Path::new(resource.trim()).join("lib").join("windows");
    let mut paths = vec![runtime];
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    let joined =
        std::env::join_paths(paths).map_err(|_| XtaskError::NonUtf8Output { program: "clang" })?;
    run.env("PATH", joined);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{components, is_cpp};

    #[test]
    fn groups_sources_and_tests_by_the_directory_that_holds_cpp() {
        let files: Vec<String> = [
            "poc/cpp-boundary/cpp/include/boundary.h",
            "poc/cpp-boundary/cpp/src/boundary.cpp",
            "poc/cpp-boundary/cpp/tests/boundary_test.cpp",
            "crates/lince-shader/cpp/src/ir.cpp",
        ]
        .map(str::to_owned)
        .to_vec();
        let refs: Vec<&String> = files.iter().collect();

        let map = components(&refs);

        let poc = &map["poc/cpp-boundary"];
        assert_eq!(poc.sources, ["poc/cpp-boundary/cpp/src/boundary.cpp"]);
        assert_eq!(poc.tests, ["poc/cpp-boundary/cpp/tests/boundary_test.cpp"]);
        assert_eq!(map["crates/lince-shader"].tests, Vec::<String>::new());
    }

    #[test]
    fn recognizes_c_and_cpp_files_only() {
        assert!(is_cpp("a/cpp/include/x.h"));
        assert!(is_cpp("a/cpp/src/x.cpp"));
        assert!(!is_cpp("a/src/x.rs"));
    }
}
