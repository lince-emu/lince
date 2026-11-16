// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `lince-a64-coverage <file.nro>...`: prints, per file, how many code words were scanned and
//! each encoding the decoder lacks with its count, most frequent first. Exits with 1 when any
//! encoding is unknown, 2 on a usage or input error.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a developer tool reports on the terminal (QUAL-43)"
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: lince-a64-coverage <file.nro>...");
        return ExitCode::from(2);
    }
    let mut gaps = false;
    for file in &files {
        let coverage = std::fs::read(file)
            .map_err(|error| format!("cannot read {file}: {error}"))
            .and_then(|bytes| {
                lince_a64_coverage::scan_nro(&bytes).map_err(|error| format!("{file}: {error}"))
            });
        let coverage = match coverage {
            Ok(coverage) => coverage,
            Err(message) => {
                eprintln!("lince-a64-coverage: {message}");
                return ExitCode::from(2);
            }
        };
        println!(
            "{file}: {} words, {} unknown ({} distinct encodings)",
            coverage.words,
            coverage.unknown_words(),
            coverage.unknown.len()
        );
        gaps |= !coverage.unknown.is_empty();
        let mut unknown: Vec<(u32, u64)> = coverage.unknown.into_iter().collect();
        unknown.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (word, count) in unknown {
            println!("  {word:#010x}  {count}");
        }
    }
    if gaps {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
