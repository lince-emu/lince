// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `lince run [--coverage] <file.nro>`: runs a homebrew NRO and prints its debug output; with
//! `--coverage`, also the unimplemented SVCs and IPC commands it reached, on stderr (T-055).
#![expect(
    clippy::print_stderr,
    reason = "the command line reports errors on the terminal (QUAL-43)"
)]

use std::process::ExitCode;

use lince_app::Options;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (options, file) = match args.as_slice() {
        [command, file] if command == "run" => (Options::default(), file),
        [command, flag, file] if command == "run" && flag == "--coverage" => {
            (Options { coverage: true }, file)
        }
        _ => return usage(),
    };
    let result = std::fs::read(file)
        .map_err(|error| format!("cannot read {file}: {error}"))
        .and_then(|nro| {
            lince_app::run_nro(&nro, &mut std::io::stdout().lock(), options)
                .map_err(|error| format!("{file}: {error}"))
        })
        .and_then(|report| {
            if !options.coverage {
                return Ok(());
            }
            lince_app::write_coverage(&mut std::io::stderr().lock(), &report)
                .map_err(|error| format!("cannot write the coverage list: {error}"))
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
    eprintln!("usage: lince run [--coverage] <file.nro>");
    ExitCode::from(2)
}
