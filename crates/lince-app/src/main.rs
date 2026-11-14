// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `lince run <file.nro>`: runs a homebrew NRO and prints its debug output.
#![expect(
    clippy::print_stderr,
    reason = "the command line reports errors on the terminal (QUAL-43)"
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, file] = args.as_slice() else {
        return usage();
    };
    if command != "run" {
        return usage();
    }
    let result = std::fs::read(file)
        .map_err(|error| format!("cannot read {file}: {error}"))
        .and_then(|nro| {
            lince_app::run_nro(&nro, &mut std::io::stdout().lock())
                .map_err(|error| format!("{file}: {error}"))
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("lince: {message}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: lince run <file.nro>");
    ExitCode::from(2)
}
