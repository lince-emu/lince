// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The B1 exit program (`tests/homebrew/hello.s`) run end to end by the interpreter: loaded by
//! `lince-loader`, it prints "hello" with `OutputDebugString` (SVC 0x27) and exits with
//! `ExitProcess` (SVC 0x07) (docs/research/nro-and-b1-svcs.md S8, S9). The SVCs are handled here
//! the way the kernel does, so the test needs no kernel.

use lince_common::GuestVa;
use lince_cpu_a64::{Exit, State, run};
use lince_loader::load_nro;
use lince_mem::{AddressSpace, SpaceConfig};

const BASE: u64 = 0x800_0000;
const OUTPUT_DEBUG_STRING: u16 = 0x27;
const EXIT_PROCESS: u16 = 0x07;
/// Far more instructions than the program runs; reaching it means it never exited.
const LIMIT: u64 = 1_000;

#[test]
fn hello_prints_hello_then_exits() {
    let file = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/homebrew/hello.nro"
    ))
    .unwrap();
    let mut mem = AddressSpace::new(SpaceConfig::for_host()).unwrap();
    let nro = load_nro(&mut mem, GuestVa::new(BASE), &file).unwrap();
    let mut state = State {
        pc: nro.entry.get(),
        ..State::default()
    };
    let mut printed = Vec::new();

    loop {
        let result = run(&mut state, &mut mem, LIMIT).unwrap();
        match result.exit {
            Exit::Svc {
                number: OUTPUT_DEBUG_STRING,
            } => {
                let mut text = vec![0; usize::try_from(state.x[1]).unwrap()];
                mem.read_bytes(GuestVa::new(state.x[0]), &mut text).unwrap();
                printed.push(String::from_utf8(text).unwrap());
            }
            Exit::Svc {
                number: EXIT_PROCESS,
            } => break,
            other @ (Exit::Svc { .. }
            | Exit::Limit
            | Exit::Undefined { .. }
            | Exit::Unimplemented { .. }
            | Exit::FetchAbort { .. }
            | Exit::PcMisaligned { .. }) => {
                panic!("unexpected exit {other:?} at pc {:#x}", state.pc)
            }
        }
    }

    assert_eq!(printed, ["hello"]);
}
