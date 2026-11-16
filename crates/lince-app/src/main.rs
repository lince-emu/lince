// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `lince run [--coverage] [--sd <dir> [--sd-writable]] <file.nro>`: runs a homebrew NRO and
//! prints its debug output; with `--coverage`, also the unimplemented SVCs and IPC commands it
//! reached, on stderr (T-055); with `--sd`, the guest's SD card is that host directory,
//! read-only unless `--sd-writable` is given too (T-064).
#![expect(
    clippy::print_stderr,
    reason = "the command line reports errors on the terminal (QUAL-43)"
)]

use std::path::PathBuf;
use std::process::ExitCode;

use lince_app::Options;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((options, file)) = parse(&args) else {
        return usage();
    };
    let result = std::fs::read(file)
        .map_err(|error| format!("cannot read {file}: {error}"))
        .and_then(|nro| {
            lince_app::run_nro(&nro, &mut std::io::stdout().lock(), &options)
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

/// `run [--coverage] [--sd <dir> [--sd-writable]] <file.nro>`, each flag at most once and
/// `--sd-writable` only with `--sd`.
fn parse(args: &[String]) -> Option<(Options, &String)> {
    let (command, mut rest) = args.split_first()?;
    if command != "run" {
        return None;
    }
    let mut options = Options::default();
    loop {
        match rest {
            [flag, tail @ ..] if flag == "--coverage" && !options.coverage => {
                options.coverage = true;
                rest = tail;
            }
            [flag, dir, tail @ ..] if flag == "--sd" && options.sd.is_none() => {
                options.sd = Some(PathBuf::from(dir));
                rest = tail;
            }
            [flag, tail @ ..] if flag == "--sd-writable" && !options.sd_writable => {
                options.sd_writable = true;
                rest = tail;
            }
            [file] if !file.starts_with("--") && (options.sd.is_some() || !options.sd_writable) => {
                return Some((options, file));
            }
            _ => return None,
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: lince run [--coverage] [--sd <dir> [--sd-writable]] <file.nro>");
    ExitCode::from(2)
}
