// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The B1 programs (`tests/homebrew/*.s`) run end to end by the interpreter: loaded by
//! `lince-loader`, each prints with `OutputDebugString` (SVC 0x27) and exits with
//! `ExitProcess` (SVC 0x07) (docs/research/nro-and-b1-svcs.md S8, S9). `hello` prints "hello";
//! the self-checking `alu`, `branch` and `memory` programs print "ok" or "fail". The SVCs are
//! handled here the way the kernel does, so the test needs no kernel.

use lince_common::GuestVa;
use lince_cpu_a64::{Exit, State, run};
use lince_loader::load_nro;
use lince_mem::{AddressSpace, SpaceConfig};

const BASE: u64 = 0x800_0000;
const OUTPUT_DEBUG_STRING: u16 = 0x27;
const EXIT_PROCESS: u16 = 0x07;
/// Far more instructions than any B1 program runs; reaching it means it never exited.
const LIMIT: u64 = 10_000;

/// Runs `tests/homebrew/<name>.nro` until `ExitProcess`; returns what it printed.
fn run_program(name: &str) -> Vec<String> {
    let file = std::fs::read(format!(
        "{}/../../tests/homebrew/{name}.nro",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read the NRO");
    let mut mem = AddressSpace::new(SpaceConfig::for_host()).expect("space");
    let nro = load_nro(&mut mem, GuestVa::new(BASE), &file).expect("load the NRO");
    let mut state = State {
        pc: nro.entry.get(),
        ..State::default()
    };
    let mut printed = Vec::new();

    loop {
        let result = run(&mut state, &mut mem, LIMIT).expect("run");
        match result.exit {
            Exit::Svc {
                number: OUTPUT_DEBUG_STRING,
            } => {
                let mut text = vec![0; usize::try_from(state.x[1]).expect("string length")];
                mem.read_bytes(GuestVa::new(state.x[0]), &mut text)
                    .expect("read the string");
                printed.push(String::from_utf8(text).expect("UTF-8 output"));
            }
            Exit::Svc {
                number: EXIT_PROCESS,
            } => return printed,
            other @ (Exit::Svc { .. }
            | Exit::Limit
            | Exit::Undefined { .. }
            | Exit::Unimplemented { .. }
            | Exit::FetchAbort { .. }
            | Exit::PcMisaligned { .. }
            | Exit::DataAbort { .. }) => {
                panic!("{name}: unexpected exit {other:?} at pc {:#x}", state.pc)
            }
        }
    }
}

#[test]
fn hello_prints_hello_then_exits() {
    assert_eq!(run_program("hello"), ["hello"]);
}

#[test]
fn alu_checks_integer_data_processing() {
    assert_eq!(run_program("alu"), ["ok"]);
}

#[test]
fn branch_sums_one_to_ten_in_a_loop() {
    assert_eq!(run_program("branch"), ["ok"]);
}

#[test]
fn memory_reads_back_every_store() {
    assert_eq!(run_program("memory"), ["ok"]);
}
