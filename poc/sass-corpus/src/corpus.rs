// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds the stored artifacts of one corpus program: its binary and its expected output.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::exec;
use crate::isa;
use crate::parse;

/// The stored form of one program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifacts {
    /// Instructions as 16-byte little-endian words, in program order.
    pub binary: Vec<u8>,
    /// Expected final memory, one `0xADDRESS 0xVALUE` line per word, ascending.
    pub expected: String,
}

/// Assembles `source` and computes its expected output on the CPU.
pub fn build(source: &str) -> Result<Artifacts, String> {
    let program = parse::parse(source).map_err(|e| e.to_string())?;
    let binary = program
        .instrs
        .iter()
        .enumerate()
        .flat_map(|(i, instr)| isa::encode(instr, i))
        .flat_map(u32::to_le_bytes)
        .collect();
    let memory = exec::run(&program).map_err(|e| e.to_string())?;
    Ok(Artifacts {
        binary,
        expected: expected_text(&memory),
    })
}

fn expected_text(memory: &BTreeMap<u64, u32>) -> String {
    memory.iter().fold(String::new(), |mut text, (addr, v)| {
        // Writing to a String cannot fail.
        let _ = writeln!(text, "{addr:#010x} {v:#010x}");
        text
    })
}
