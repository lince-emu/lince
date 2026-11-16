// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lists the A64 encodings an executable uses that the decoder's description table lacks.
//!
//! # Role
//! The coverage tool of ADR-0004: every word of an executable's code segment goes through the
//! shared decoder (`lince-cpu-a64`, the C API of ADR-0031), and each word the table does not
//! decode is counted. An undecoded encoding is a reported coverage gap, never a guessed one.
//! The tool reads a file the user points it at and prints only encodings and counts; nothing
//! of the file is stored (LEGAL-50).
//!
//! # References
//! docs/adr/0004-guest-cpu-model.md · docs/research/nro-and-b1-svcs.md (N1, N2)

use std::collections::BTreeMap;
use std::ops::Range;

use lince_loader::{LoaderError, NroHeader};

/// NRO bytes that sit in the code segment but are not code: the module-header location after
/// the start branch (N1) and the `NRO0` header (N2).
const NRO_NOT_CODE: Range<usize> = 0x4..0x90;
const WORD: usize = 4;

/// What a scan found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Instruction words scanned.
    pub words: u64,
    /// Each undecoded word and how many times it occurs.
    pub unknown: BTreeMap<u32, u64>,
}

impl Coverage {
    /// Undecoded words, counting repeats.
    pub fn unknown_words(&self) -> u64 {
        self.unknown.values().sum()
    }

    /// Adds the little-endian instruction words of the code segment `code`; a trailing partial
    /// word is ignored.
    pub fn scan(&mut self, code: &[u8]) {
        self.scan_except(code, &(0..0));
    }

    /// Like [`Coverage::scan`], but bytes in `skip` and the literal-pool data that the
    /// segment's own literal loads read are not instructions and are not scanned.
    fn scan_except(&mut self, code: &[u8], skip: &Range<usize>) {
        let (chunks, _partial) = code.as_chunks::<WORD>();
        let words: Vec<u32> = chunks
            .iter()
            .map(|chunk| u32::from_le_bytes(*chunk))
            .collect();
        let mut is_code: Vec<bool> = (0..words.len())
            .map(|index| !skip.contains(&(index * WORD)))
            .collect();
        let literals: Vec<(usize, usize)> = words
            .iter()
            .enumerate()
            .filter(|&(index, _)| is_code[index])
            .filter_map(|(index, &word)| literal_load(word, index))
            .collect();
        for (target, bytes) in literals {
            let end = target.saturating_add(bytes.div_ceil(WORD)).min(words.len());
            if let Some(data) = is_code.get_mut(target..end) {
                data.fill(false);
            }
        }
        for (&word, _) in words.iter().zip(&is_code).filter(|(_, code)| **code) {
            self.words += 1;
            if lince_cpu_a64::decode(word).is_none() {
                *self.unknown.entry(word).or_default() += 1;
            }
        }
    }
}

/// When the word at word index `index` is a decoded load-register (literal) instruction, the
/// word index it reads from and the bytes it reads. Arm ARM C4.1.4: bits 29:27 = 0b011 and
/// 25:24 = 0b00; imm19 (bits 23:5) is a word offset from the instruction; opc (bits 31:30) and
/// V (bit 26) give the size, and opc = 0b11 with V = 0 is PRFM, which reads nothing.
fn literal_load(word: u32, index: usize) -> Option<(usize, usize)> {
    const CLASS_MASK: u32 = 0x3b00_0000;
    const CLASS: u32 = 0x1800_0000;
    if word & CLASS_MASK != CLASS || lince_cpu_a64::decode(word).is_none() {
        return None;
    }
    let bytes = match (word >> 30, (word >> 26) & 1) {
        (0b01, _) => 8,
        (0b10, 1) => 16,
        (0b11, 0) => return None,
        _ => 4,
    };
    let imm19 = (word >> 5) & 0x7_ffff;
    let sign = if imm19 & 0x4_0000 == 0 { 0 } else { 0x8_0000 };
    let offset = i64::from(imm19) - sign;
    let target = i64::try_from(index).ok()?.checked_add(offset)?;
    Some((usize::try_from(target).ok()?, bytes))
}

/// Scans the code segment of the NRO `file`, skipping the header bytes that live in it.
pub fn scan_nro(file: &[u8]) -> Result<Coverage, LoaderError> {
    let header = NroHeader::parse(file)?;
    let start = header.text.offset as usize;
    let text = &file[start..start + header.text.size as usize];
    let mut coverage = Coverage::default();
    coverage.scan_except(text, &NRO_NOT_CODE);
    Ok(coverage)
}
