// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The coverage tool on the checked-in test programs (T-029, B2) and on made-up code.

use std::path::PathBuf;

use lince_a64_coverage::{Coverage, scan_nro};

const PROGRAMS: [&str; 5] = ["alu", "b2", "branch", "hello", "memory"];
/// `nop`, decoded by the table.
const NOP: u32 = 0xd503_201f;
/// A system-register move with op0 = 0b00, which the Arm ARM leaves unallocated.
const UNALLOCATED: u32 = 0xd510_0000;

fn nro(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/homebrew")
        .join(format!("{name}.nro"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

#[test]
fn every_test_program_decodes_without_unknown_encodings() {
    for name in PROGRAMS {
        let coverage = scan_nro(&nro(name)).expect("scan");
        assert!(coverage.words > 0, "{name}: no code scanned");
        assert!(
            coverage.unknown.is_empty(),
            "{name}: {:x?}",
            coverage.unknown
        );
    }
}

#[test]
fn scan_counts_each_unknown_encoding_and_how_often_it_occurs() {
    let mut coverage = Coverage::default();
    coverage.scan(&bytes(&[NOP, UNALLOCATED, NOP, UNALLOCATED]));
    assert_eq!(coverage.words, 4);
    assert_eq!(coverage.unknown_words(), 2);
    assert_eq!(
        coverage.unknown.into_iter().collect::<Vec<_>>(),
        [(UNALLOCATED, 2)]
    );
}

#[test]
fn scan_ignores_a_trailing_partial_word() {
    let mut coverage = Coverage::default();
    coverage.scan(&[0x1f, 0x20, 0x03]);
    assert_eq!(coverage, Coverage::default());
}

#[test]
fn scan_nro_skips_the_header_but_not_the_code_after_it() {
    let mut file = nro("hello");
    let first_code_word = 0x90;
    file[first_code_word..first_code_word + 4].copy_from_slice(&UNALLOCATED.to_le_bytes());
    let coverage = scan_nro(&file).expect("scan");
    assert_eq!(coverage.unknown.get(&UNALLOCATED), Some(&1));
}

#[test]
fn scan_nro_rejects_a_file_that_is_not_an_nro() {
    assert!(scan_nro(&[0; 0x100]).is_err());
}

#[test]
fn scan_skips_the_literal_pool_words_a_literal_load_reads() {
    // ldr x0, #8 (LDR literal, 64-bit) reads the two words at +8; ldr w0, #8 reads one.
    let load_x0 = 0x5800_0040;
    let load_w0_narrow = 0x1800_0040;

    let mut wide = Coverage::default();
    wide.scan(&bytes(&[load_x0, NOP, UNALLOCATED, UNALLOCATED]));
    assert_eq!(wide.words, 2);
    assert_eq!(wide.unknown_words(), 0);

    let mut narrow = Coverage::default();
    narrow.scan(&bytes(&[load_w0_narrow, NOP, UNALLOCATED, UNALLOCATED]));
    assert_eq!(narrow.words, 3);
    assert_eq!(
        narrow.unknown.into_iter().collect::<Vec<_>>(),
        [(UNALLOCATED, 1)]
    );
}
