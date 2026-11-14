// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `elf2nro <input.elf> <output.nro>`: the command-line form of [`elf2nro::convert`].
#![expect(
    clippy::print_stderr,
    reason = "a developer tool reports errors on the terminal (QUAL-43)"
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: elf2nro <input.elf> <output.nro>");
        return ExitCode::from(2);
    };
    let result = std::fs::read(input)
        .map_err(|error| format!("cannot read {input}: {error}"))
        .and_then(|elf| elf2nro::convert(&elf).map_err(|error| format!("{input}: {error}")))
        .and_then(|nro| {
            std::fs::write(output, nro).map_err(|error| format!("cannot write {output}: {error}"))
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("elf2nro: {message}");
            ExitCode::FAILURE
        }
    }
}
