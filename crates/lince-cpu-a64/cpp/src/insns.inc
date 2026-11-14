// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The A64 instruction description table (ADR-0004): the only source of A64 decoding in Lince.
// The decoder is generated from it at compile time (decode.cpp); the interpreter and the JIT
// tiers share that decoder.
//
// LCA_INSN(name, pattern...) declares one instruction form with one to four encoding patterns.
// A pattern lists bits 31 down to 0: '0' and '1' must match, 'x' is an operand bit, spaces are
// ignored. Reserved field values that one pattern cannot exclude (for example shift = 0b11) are
// excluded by listing the valid sub-patterns, so a reserved encoding stays unknown. No two
// patterns may match the same word; decode.cpp checks this at compile time.
//
// Encodings come from the Arm Architecture Reference Manual for A-profile (public, LEGAL-11),
// chapter C4 "A64 Instruction Set Encoding"; names follow the manual's instruction titles.
// B1 scope: the forms the B1 test programs use (tests/homebrew/*.s); T-040 completes the table.

// Branches, exception generation and system
LCA_INSN(B, "000101 xxxxxxxxxxxxxxxxxxxxxxxxxx")
LCA_INSN(BL, "100101 xxxxxxxxxxxxxxxxxxxxxxxxxx")
LCA_INSN(B_COND, "0101010 0 xxxxxxxxxxxxxxxxxxx 0 xxxx")
LCA_INSN(CBZ, "x 011010 0 xxxxxxxxxxxxxxxxxxx xxxxx")
LCA_INSN(CBNZ, "x 011010 1 xxxxxxxxxxxxxxxxxxx xxxxx")
LCA_INSN(TBZ, "x 011011 0 xxxxx xxxxxxxxxxxxxx xxxxx")
LCA_INSN(TBNZ, "x 011011 1 xxxxx xxxxxxxxxxxxxx xxxxx")
LCA_INSN(RET, "1101011 0010 11111 000000 xxxxx 00000")
LCA_INSN(SVC, "11010100 000 xxxxxxxxxxxxxxxx 000 01")

// Data processing, immediate. 32-bit forms (sf = 0) require N = 0 and, for the
// bitfield forms, immr and imms below 32; move-wide 32-bit forms require hw < 2.
LCA_INSN(ADRP, "1 xx 10000 xxxxxxxxxxxxxxxxxxx xxxxx")
LCA_INSN(ADD_IMM, "x 0 0 100010 x xxxxxxxxxxxx xxxxx xxxxx")
LCA_INSN(ADDS_IMM, "x 0 1 100010 x xxxxxxxxxxxx xxxxx xxxxx")
LCA_INSN(SUB_IMM, "x 1 0 100010 x xxxxxxxxxxxx xxxxx xxxxx")
LCA_INSN(SUBS_IMM, "x 1 1 100010 x xxxxxxxxxxxx xxxxx xxxxx")
// The bitmask validity of N:immr:imms (some all-ones imms values are reserved) cannot be
// expressed as bit patterns; the interpreter (T-042) rejects those values when it decodes the mask.
LCA_INSN(AND_IMM, "1 00 100100 x xxxxxx xxxxxx xxxxx xxxxx",
         "0 00 100100 0 xxxxxx xxxxxx xxxxx xxxxx")
LCA_INSN(ORR_IMM, "1 01 100100 x xxxxxx xxxxxx xxxxx xxxxx",
         "0 01 100100 0 xxxxxx xxxxxx xxxxx xxxxx")
LCA_INSN(MOVZ, "1 10 100101 xx xxxxxxxxxxxxxxxx xxxxx", "0 10 100101 0x xxxxxxxxxxxxxxxx xxxxx")
LCA_INSN(MOVK, "1 11 100101 xx xxxxxxxxxxxxxxxx xxxxx", "0 11 100101 0x xxxxxxxxxxxxxxxx xxxxx")
LCA_INSN(UBFM, "1 10 100110 1 xxxxxx xxxxxx xxxxx xxxxx",
         "0 10 100110 0 0xxxxx 0xxxxx xxxxx xxxxx")

// Data processing, register: shifted-register forms. Add/subtract forbid
// shift = 0b11; 32-bit forms require imm6 below 32.
LCA_INSN(ADD_REG, "1 0 0 01011 0x 0 xxxxx xxxxxx xxxxx xxxxx",
         "1 0 0 01011 10 0 xxxxx xxxxxx xxxxx xxxxx",
         "0 0 0 01011 0x 0 xxxxx 0xxxxx xxxxx xxxxx",
         "0 0 0 01011 10 0 xxxxx 0xxxxx xxxxx xxxxx")
LCA_INSN(SUB_REG, "1 1 0 01011 0x 0 xxxxx xxxxxx xxxxx xxxxx",
         "1 1 0 01011 10 0 xxxxx xxxxxx xxxxx xxxxx",
         "0 1 0 01011 0x 0 xxxxx 0xxxxx xxxxx xxxxx",
         "0 1 0 01011 10 0 xxxxx 0xxxxx xxxxx xxxxx")
LCA_INSN(SUBS_REG, "1 1 1 01011 0x 0 xxxxx xxxxxx xxxxx xxxxx",
         "1 1 1 01011 10 0 xxxxx xxxxxx xxxxx xxxxx",
         "0 1 1 01011 0x 0 xxxxx 0xxxxx xxxxx xxxxx",
         "0 1 1 01011 10 0 xxxxx 0xxxxx xxxxx xxxxx")
LCA_INSN(ORR_REG, "1 01 01010 xx 0 xxxxx xxxxxx xxxxx xxxxx",
         "0 01 01010 xx 0 xxxxx 0xxxxx xxxxx xxxxx")
LCA_INSN(ORN_REG, "1 01 01010 xx 1 xxxxx xxxxxx xxxxx xxxxx",
         "0 01 01010 xx 1 xxxxx 0xxxxx xxxxx xxxxx")
LCA_INSN(EOR_REG, "1 10 01010 xx 0 xxxxx xxxxxx xxxxx xxxxx",
         "0 10 01010 xx 0 xxxxx 0xxxxx xxxxx xxxxx")

// Loads and stores. opc = 0b01 of the pair forms is STGP/LDPSW, not STP/LDP.
LCA_INSN(LDR_LIT, "0x 011 0 00 xxxxxxxxxxxxxxxxxxx xxxxx")
LCA_INSN(LDR_UIMM, "1x 111 0 01 01 xxxxxxxxxxxx xxxxx xxxxx")
LCA_INSN(STR_UIMM, "1x 111 0 01 00 xxxxxxxxxxxx xxxxx xxxxx")
LCA_INSN(LDRB_UIMM, "00 111 0 01 01 xxxxxxxxxxxx xxxxx xxxxx")
LCA_INSN(LDRH_UIMM, "01 111 0 01 01 xxxxxxxxxxxx xxxxx xxxxx")
LCA_INSN(STRB_PRE, "00 111 0 00 00 0 xxxxxxxxx 11 xxxxx xxxxx")
LCA_INSN(LDRB_POST, "00 111 0 00 01 0 xxxxxxxxx 01 xxxxx xxxxx")
LCA_INSN(STP_OFF, "x0 101 0 010 0 xxxxxxx xxxxx xxxxx xxxxx")
LCA_INSN(LDP_OFF, "x0 101 0 010 1 xxxxxxx xxxxx xxxxx xxxxx")
