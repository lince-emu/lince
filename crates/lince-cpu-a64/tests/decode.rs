// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoder tests through the public API.
//!
//! The known encodings are every distinct instruction of the B1 programs
//! (`tests/homebrew/{alu,branch,memory,hello}.s`) as LLVM assembles them (clang
//! `--target=aarch64-none-elf`, listed with `llvm-objdump -d`); the forms they map to follow the
//! Arm Architecture Reference Manual, chapter C4.

use lince_cpu_a64::decode;

fn name_of(word: u32) -> Option<String> {
    decode(word).map(lince_cpu_a64::Insn::name)
}

#[test]
fn decode_finds_the_form_when_the_word_is_a_b1_branch_or_system_instruction() {
    let cases = [
        (0x1400_0001, "B", "b #4"),
        (0x9400_0009, "BL", "bl #36"),
        (0x5400_0361, "B_COND", "b.ne"),
        (0x5400_0143, "B_COND", "b.lo"),
        (0x5400_0080, "B_COND", "b.eq"),
        (0xb400_0193, "CBZ", "cbz x19"),
        (0xb500_00d1, "CBNZ", "cbnz x17"),
        (0xb5ff_ffb4, "CBNZ", "cbnz x20, backwards"),
        (0x3600_0153, "TBZ", "tbz w19, #0"),
        (0x3718_0173, "TBNZ", "tbnz w19, #3"),
        (0xd65f_03c0, "RET", "ret"),
        (0xd400_04e1, "SVC", "svc #0x27"),
        (0xd400_00e1, "SVC", "svc #0x7"),
    ];
    for (word, form, text) in cases {
        assert_eq!(
            name_of(word).as_deref(),
            Some(form),
            "{text} ({word:#010x})"
        );
    }
}

#[test]
fn decode_finds_the_form_when_the_word_is_a_b1_data_processing_instruction() {
    let cases = [
        (0x9000_0001, "ADRP", "adrp x1"),
        (0x9100_5c85, "ADD_IMM", "add x5, x4, #23"),
        (0x3100_05f0, "ADDS_IMM", "adds w16, w15, #1"),
        (0xd100_0694, "SUB_IMM", "sub x20, x20, #1"),
        (0xf101_cce8, "SUBS_IMM", "subs x8, x7, #115"),
        (0x7101_dc9f, "SUBS_IMM", "cmp w4, #0x77"),
        (0x927e_0d2a, "AND_IMM", "and x10, x9, #0x3c"),
        (0xb240_0d4b, "ORR_IMM", "orr x11, x10, #0xf"),
        (0xd2a2_4682, "MOVZ", "mov x2, #0x12340000"),
        (0x528a_acc7, "MOVZ", "mov w7, #0x5566"),
        (0xf28a_cf02, "MOVK", "movk x2, #0x5678"),
        (0xd37c_ed8d, "UBFM", "lsl x13, x12, #4"),
        (0xd348_fdae, "UBFM", "lsr x14, x13, #8"),
        (0x8b06_08c7, "ADD_REG", "add x7, x6, x6, lsl #2"),
        (0xcb04_00a6, "SUB_REG", "sub x6, x5, x4"),
        (0xeb03_005f, "SUBS_REG", "cmp x2, x3"),
        (0x6b07_00bf, "SUBS_REG", "cmp w5, w7"),
        (0xeb4d_81df, "SUBS_REG", "cmp x14, x13, lsr #32"),
        (0xaa14_03e0, "ORR_REG", "mov x0, x20"),
        (0x2a3f_03ef, "ORN_REG", "mvn w15, wzr"),
        (0xca09_016c, "EOR_REG", "eor x12, x11, x9"),
    ];
    for (word, form, text) in cases {
        assert_eq!(
            name_of(word).as_deref(),
            Some(form),
            "{text} ({word:#010x})"
        );
    }
}

#[test]
fn decode_finds_the_form_when_the_word_is_a_b1_load_or_store() {
    let cases = [
        (0x5800_0443, "LDR_LIT", "ldr x3, literal"),
        (0xf940_0022, "LDR_UIMM", "ldr x2, [x1]"),
        (0xb940_0823, "LDR_UIMM", "ldr w3, [x1, #8]"),
        (0xb900_0505, "STR_UIMM", "str w5, [x8, #4]"),
        (0x3940_0424, "LDRB_UIMM", "ldrb w4, [x1, #1]"),
        (0x7940_0425, "LDRH_UIMM", "ldrh w5, [x1, #2]"),
        (0x3801_0d04, "STRB_PRE", "strb w4, [x8, #16]!"),
        (0x385f_050c, "LDRB_POST", "ldrb w12, [x8], #-16"),
        (0xa900_0d02, "STP_OFF", "stp x2, x3, [x8]"),
        (0xa940_2d0a, "LDP_OFF", "ldp x10, x11, [x8]"),
    ];
    for (word, form, text) in cases {
        assert_eq!(
            name_of(word).as_deref(),
            Some(form),
            "{text} ({word:#010x})"
        );
    }
}

#[test]
fn decode_reports_unknown_when_a_field_holds_a_reserved_value() {
    let cases = [
        (0x0000_0000, "udf #0 (permanently undefined)"),
        (0x8bc0_0000, "ADD (shifted register) with shift = 0b11"),
        (0x0b00_8000, "32-bit ADD (shifted register) with imm6 = 32"),
        (0x2a00_8000, "32-bit ORR (shifted register) with imm6 = 32"),
        (0x52c0_0000, "32-bit MOVZ with hw = 2"),
        (0x1240_0000, "32-bit AND (immediate) with N = 1"),
        (0x5340_0000, "32-bit UBFM with N = 1"),
        (0x5320_0000, "32-bit UBFM with immr = 32"),
        (0xd65f_03c1, "RET with nonzero low bits"),
        (0xd400_0005, "SVC opcode with an unallocated low field"),
        (0x6900_0000, "STGP, the opc = 0b01 pair form"),
    ];
    for (word, text) in cases {
        assert_eq!(name_of(word), None, "{text} ({word:#010x})");
    }
}

#[test]
fn decode_reports_unknown_when_the_form_is_not_in_the_b1_table() {
    // Valid A64 instructions outside the B1 scope stay unknown until the table has them.
    let cases = [
        (0xd503_201f, "nop"),
        (0x9b02_7c20, "mul x0, x1, x2"),
        (0x1e20_2800, "fadd s0, s0, s0"),
    ];
    for (word, text) in cases {
        assert_eq!(name_of(word), None, "{text} ({word:#010x})");
    }
}

#[test]
fn insn_formats_as_its_name() {
    let insn = decode(0xd65f_03c0).expect("ret decodes");

    assert_eq!(insn.to_string(), "RET");
    assert_eq!(format!("{insn:?}"), "RET");
}
