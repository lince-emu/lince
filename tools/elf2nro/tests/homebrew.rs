// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Checks the checked-in B1 test NROs against the header layout of
//! docs/research/nro-and-b1-svcs.md (N1-N4), and the converter's rejection of bad input.

use std::fs;
use std::path::PathBuf;

use elf2nro::{ConvertError, convert};

const PAGE: u32 = 0x1000;
const PROGRAMS: [&str; 4] = ["alu", "branch", "hello", "memory"];

fn nro(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/homebrew")
        .join(format!("{name}.nro"));
    fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn word(file: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]])
}

#[test]
fn every_program_has_a_valid_nro_header() {
    for name in PROGRAMS {
        let file = nro(name);
        let len = u32::try_from(file.len()).unwrap();

        assert_eq!(&file[0x10..0x14], b"NRO0", "{name}: magic (N2)");
        assert_eq!(word(&file, 0x14), 0, "{name}: version 0 (N2, O1)");
        assert_eq!(word(&file, 0x18), len, "{name}: total size (N2)");
        assert_eq!(word(&file, 0x1C), 0, "{name}: flags 0 (N2)");
        let [text, text_size, ro, ro_size, data, data_size] =
            [0x20, 0x24, 0x28, 0x2C, 0x30, 0x34].map(|at| word(&file, at));
        assert_eq!(text, 0, "{name}: text at 0 (N1, entry at file offset 0)");
        assert!(text_size > 0, "{name}: text not empty");
        assert_eq!(ro, text + text_size, "{name}: ro follows text (N3)");
        assert_eq!(data, ro + ro_size, "{name}: data follows ro (N3)");
        assert_eq!(
            data + data_size,
            len,
            "{name}: data ends the file, no asset block (N5)"
        );
        for value in [text_size, ro, ro_size, data, data_size] {
            assert_eq!(value % PAGE, 0, "{name}: page-aligned segments (O1)");
        }
        assert!(
            file[0x3C..0x90].iter().all(|b| *b == 0),
            "{name}: zero tail (N4)"
        );
        assert!(
            file[0x04..0x10].iter().all(|b| *b == 0),
            "{name}: zero start block (N1)"
        );
        let branch = word(&file, 0);
        assert_eq!(branch >> 26, 0b00_0101, "{name}: starts with B (N1)");
        assert!(
            (branch & 0x03FF_FFFF) * 4 >= 0x90,
            "{name}: branch skips the header"
        );
    }
}

#[test]
fn hello_keeps_its_message_in_ro_and_memory_has_bss() {
    let hello = nro("hello");
    let ro = word(&hello, 0x28) as usize;
    assert_eq!(&hello[ro..ro + 5], b"hello");

    let memory = nro("memory");
    assert_eq!(word(&memory, 0x34), PAGE, "data segment present");
}

#[test]
fn input_that_is_not_an_aarch64_elf_is_rejected() {
    assert!(matches!(convert(b""), Err(ConvertError::BadElf(_))));
    assert!(matches!(convert(&[0; 0x40]), Err(ConvertError::BadElf(_))));

    let mut x86 = vec![0; 0x40];
    x86[..6].copy_from_slice(b"\x7fELF\x02\x01");
    x86[0x12] = 62;
    assert!(matches!(
        convert(&x86),
        Err(ConvertError::BadElf("not AArch64"))
    ));

    let mut no_segments = x86;
    no_segments[0x12] = 183;
    no_segments[0x36] = 56;
    assert!(matches!(
        convert(&no_segments),
        Err(ConvertError::BadLayout(_))
    ));

    no_segments[0x38] = 1;
    no_segments[0x20] = 0x40;
    assert!(matches!(
        convert(&no_segments),
        Err(ConvertError::BadElf(_))
    ));
}
