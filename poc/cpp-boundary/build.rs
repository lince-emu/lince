// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds the C++20 static library with the `cc` crate (ADR-0030): `clang-cl` on Windows,
//! `clang++` elsewhere, without exceptions or RTTI (ARCH-51).
//!
//! Compiler discovery (a PoC-11 alternative): `CXX` when set, which is how `cc` normally
//! chooses; otherwise the LLVM installer's default directory on Windows, then `PATH`.

use std::path::Path;

/// Where the official LLVM installer puts its tools on Windows.
const WINDOWS_LLVM_BIN: &str = "C:\\Program Files\\LLVM\\bin";

fn main() {
    // Build scripts learn the target only through Cargo's environment.
    let windows = std::env::var_os("CARGO_CFG_TARGET_OS").is_some_and(|os| os == "windows");
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .include("cpp/include")
        .file("cpp/src/boundary.cpp")
        .warnings(true)
        .warnings_into_errors(true);
    if std::env::var_os("CXX").is_none() {
        build.compiler(default_compiler(windows));
    }
    if windows {
        build.flag("/GR-").flag("/EHs-c-");
    } else {
        build.flag("-fno-exceptions").flag("-fno-rtti");
    }
    build.compile("lince_poc_boundary");
    println!("cargo::rerun-if-changed=cpp");
    println!("cargo::rerun-if-env-changed=CXX");
}

fn default_compiler(windows: bool) -> String {
    if !windows {
        return "clang++".to_owned();
    }
    let installed = Path::new(WINDOWS_LLVM_BIN).join("clang-cl.exe");
    if installed.exists() {
        installed.to_string_lossy().into_owned()
    } else {
        "clang-cl".to_owned()
    }
}
