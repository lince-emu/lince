// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Differential tests of the Tier A interpreter against the Arm specification.
//!
//! Each case runs one instruction (assembled by clang `--target=aarch64-none-elf` and listed
//! with `llvm-objdump -d`) and compares the resulting state with values worked out by hand
//! from the instruction's pseudocode in the Arm Architecture Reference Manual. The host-CPU
//! comparison ADR-0004 allows on ARM hosts is not wired yet; these tests run on every host.

use lince_common::GuestVa;
use lince_cpu_a64::{Exit, Run, State, run};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};

const CODE: u64 = 0x1_0000;
const RW: Permission = Permission::READ.union(Permission::WRITE);
const RX: Permission = Permission::READ.union(Permission::EXECUTE);
const N: u32 = 1 << 31;
const Z: u32 = 1 << 30;
const C: u32 = 1 << 29;
const V: u32 = 1 << 28;

/// An address space with `words` at `CODE`, read-execute.
fn space_with(words: &[u32]) -> AddressSpace {
    let mut mem = AddressSpace::new(SpaceConfig::for_host()).expect("space");
    mem.map(GuestVa::new(CODE), GUEST_PAGE_SIZE, RW)
        .expect("map code");
    for (i, word) in (0u64..).zip(words) {
        mem.write(GuestVa::new(CODE + 4 * i), *word)
            .expect("write code");
    }
    mem.protect(GuestVa::new(CODE), GUEST_PAGE_SIZE, RX)
        .expect("protect code");
    mem
}

/// Runs the single instruction `word` at `CODE` from `state`; returns the run and the state.
fn step(word: u32, state: &State) -> (Run, State) {
    let mut mem = space_with(&[word]);
    let mut state = State {
        pc: CODE,
        ..state.clone()
    };
    let result = run(&mut state, &mut mem, 1).expect("run");
    (result, state)
}

fn with_x(pairs: &[(usize, u64)]) -> State {
    let mut state = State::default();
    for &(n, value) in pairs {
        state.x[n] = value;
    }
    state
}

/// Runs `word` and returns the new pc, checking that it completed as one instruction.
fn pc_after(word: u32, state: &State) -> u64 {
    let (result, after) = step(word, state);
    assert_eq!(
        result,
        Run {
            exit: Exit::Limit,
            executed: 1
        },
        "{word:#010x}"
    );
    after.pc
}

#[test]
fn b_and_bl_branch_relative_and_bl_links() {
    assert_eq!(pc_after(0x1400_0002, &State::default()), CODE + 8, "b +8");
    assert_eq!(pc_after(0x17ff_ffff, &State::default()), CODE - 4, "b -4");

    let (_, after) = step(0x9400_0004, &State::default()); // bl +16
    assert_eq!(after.pc, CODE + 16);
    assert_eq!(after.x[30], CODE + 4);
}

#[test]
fn ret_jumps_to_x30_or_the_named_register() {
    assert_eq!(
        pc_after(0xd65f_03c0, &with_x(&[(30, 0x4000)])),
        0x4000,
        "ret"
    );
    assert_eq!(
        pc_after(0xd65f_00a0, &with_x(&[(5, 0x8000)])),
        0x8000,
        "ret x5"
    );
}

#[test]
fn b_cond_branches_only_when_the_condition_holds() {
    let cases = [
        (0x5400_0040, Z, true, "b.eq with Z"),
        (0x5400_0040, 0, false, "b.eq without Z"),
        (0x5400_0041, Z, false, "b.ne with Z"),
        (0x5400_004b, N, true, "b.lt with N != V"),
        (0x5400_004b, N | V, false, "b.lt with N == V"),
        (0x5400_004c, 0, true, "b.gt with Z clear and N == V"),
        (0x5400_004c, Z, false, "b.gt with Z"),
        (0x5400_004e, N | Z | C | V, true, "b.al"),
        (0x5400_004f, 0, true, "b.nv (also always)"),
    ];
    for (word, nzcv, taken, text) in cases {
        let state = State {
            nzcv,
            ..State::default()
        };
        let expected = if taken { CODE + 8 } else { CODE + 4 };
        assert_eq!(pc_after(word, &state), expected, "{text}");
    }
}

#[test]
fn cbz_and_cbnz_test_the_register_at_its_width() {
    let upper_only = with_x(&[(1, 0x1_0000_0000)]);
    assert_eq!(
        pc_after(0x3400_0041, &upper_only),
        CODE + 8,
        "cbz w1: W is zero"
    );
    assert_eq!(
        pc_after(0xb500_0041, &upper_only),
        CODE + 8,
        "cbnz x1: X is not"
    );
    assert_eq!(
        pc_after(0x3400_0041, &with_x(&[(1, 1)])),
        CODE + 4,
        "cbz w1 = 1"
    );
}

#[test]
fn tbz_and_tbnz_test_one_bit() {
    let top = with_x(&[(1, 1 << 63)]);
    assert_eq!(
        pc_after(0xb6f8_0041, &top),
        CODE + 4,
        "tbz x1, #63 with bit set"
    );
    assert_eq!(
        pc_after(0xb6f8_0041, &State::default()),
        CODE + 8,
        "tbz x1, #63 clear"
    );
    assert_eq!(
        pc_after(0x3700_0041, &with_x(&[(1, 1)])),
        CODE + 8,
        "tbnz w1, #0 set"
    );
    assert_eq!(
        pc_after(0x3700_0041, &with_x(&[(1, 2)])),
        CODE + 4,
        "tbnz w1, #0 clear"
    );
}

#[test]
fn svc_completes_and_exits_with_its_number() {
    let (result, after) = step(0xd400_04e1, &State::default()); // svc #0x27

    assert_eq!(
        result,
        Run {
            exit: Exit::Svc { number: 0x27 },
            executed: 1
        }
    );
    assert_eq!(after.pc, CODE + 4);
}

#[test]
fn adrp_adds_a_page_offset_to_the_page_of_pc() {
    let mut mem = space_with(&[0xd503_201f, 0xf000_0000]); // (any word), adrp x0, +3 pages
    let mut state = State {
        pc: CODE + 4,
        ..State::default()
    };

    run(&mut state, &mut mem, 1).unwrap();

    assert_eq!(state.x[0], CODE + 0x3000);
}

#[test]
fn adr_adds_a_signed_byte_offset_to_pc_and_nop_does_nothing() {
    let (_, after) = step(0x1000_7b60, &State::default()); // adr x0, #0xf6c
    assert_eq!(after.x[0], CODE + 0xf6c);
    let (_, after) = step(0x10ff_fffe, &State::default()); // adr x30, #-4
    assert_eq!(after.x[30], CODE - 4);

    let before = with_x(&[(0, 7)]);
    let (_, after) = step(0xd503_201f, &before); // nop
    assert_eq!(
        after,
        State {
            pc: CODE + 4,
            ..before
        }
    );
}

#[test]
fn add_and_sub_immediate_use_sp_for_register_31() {
    let (_, after) = step(0x9140_0441, &with_x(&[(2, 5)])); // add x1, x2, #1, lsl #12
    assert_eq!(after.x[1], 0x1005);

    let sp = State {
        sp: 0x7000,
        ..State::default()
    };
    let (_, after) = step(0x9100_43ff, &sp); // add sp, sp, #16
    assert_eq!(after.sp, 0x7010);
    let (_, after) = step(0x9100_03e0, &sp); // mov x0, sp
    assert_eq!(after.x[0], 0x7000);
}

#[test]
fn a_32_bit_result_clears_the_upper_half() {
    let (_, after) = step(0x5100_0420, &with_x(&[(0, u64::MAX), (1, 0)])); // sub w0, w1, #1
    assert_eq!(after.x[0], 0xffff_ffff);
}

#[test]
fn adds_and_subs_set_the_flags_of_add_with_carry() {
    let cases = [
        (
            0xb100_0420,
            u64::MAX,
            0,
            Z | C,
            "adds x0, x1, #1: wraps to zero",
        ),
        (
            0x3100_0420,
            0x7fff_ffff,
            0x8000_0000,
            N | V,
            "adds w0, w1, #1: signed overflow",
        ),
        (0xf100_0420, 1, 0, Z | C, "subs x0, x1, #1: equal"),
        (0xf100_0420, 0, u64::MAX, N, "subs x0, x1, #1: borrow"),
    ];
    for (word, x1, result, nzcv, text) in cases {
        let (_, after) = step(word, &with_x(&[(1, x1)]));
        assert_eq!((after.x[0], after.nzcv), (result, nzcv), "{text}");
    }
}

#[test]
fn cmp_writes_the_zero_register_not_sp() {
    let state = State {
        sp: 0x7000,
        ..with_x(&[(1, 1)])
    };
    let (_, after) = step(0xf100_083f, &state); // cmp x1, #2 (subs xzr, x1, #2)

    assert_eq!(after.sp, 0x7000);
    assert_eq!(after.nzcv, N);
}

#[test]
fn movz_and_movk_place_sixteen_bits() {
    let (_, after) = step(0xd2e2_4680, &with_x(&[(0, u64::MAX)])); // movz x0, #0x1234, lsl #48
    assert_eq!(after.x[0], 0x1234_0000_0000_0000);

    let start = with_x(&[(0, 0x1111_2222_3333_4444)]);
    let (_, after) = step(0xf2b7_dde0, &start); // movk x0, #0xbeef, lsl #16
    assert_eq!(after.x[0], 0x1111_2222_beef_4444);

    let (_, after) = step(0x52bf_ffe0, &State::default()); // movz w0, #0xffff, lsl #16
    assert_eq!(after.x[0], 0xffff_0000);

    let (_, after) = step(0x7280_0020, &with_x(&[(0, u64::MAX)])); // movk w0, #1
    assert_eq!(after.x[0], 0xffff_0001);
}

#[test]
fn run_stops_before_an_undefined_word() {
    let (result, after) = step(0x0000_0000, &State::default()); // udf #0
    assert_eq!(
        result.exit,
        Exit::Undefined {
            pc: GuestVa::new(CODE)
        }
    );
    assert_eq!((result.executed, after.pc), (0, CODE));
}

#[test]
fn fetching_outside_executable_memory_aborts() {
    let mut mem = space_with(&[0x1400_0000]);
    mem.map(GuestVa::new(0x2_0000), GUEST_PAGE_SIZE, RW)
        .unwrap();
    for pc in [0x2_0000, 0x3_0000] {
        let mut state = State {
            pc,
            ..State::default()
        };
        let result = run(&mut state, &mut mem, 1).unwrap();
        assert_eq!(
            result,
            Run {
                exit: Exit::FetchAbort {
                    pc: GuestVa::new(pc)
                },
                executed: 0
            },
            "{pc:#x}"
        );
    }
}

#[test]
fn a_branch_to_a_misaligned_address_stops_before_fetching() {
    let mut mem = space_with(&[0xd65f_03c0]); // ret
    let mut state = State {
        pc: CODE,
        ..with_x(&[(30, CODE + 2)])
    };

    let result = run(&mut state, &mut mem, 2).unwrap();

    assert_eq!(
        result,
        Run {
            exit: Exit::PcMisaligned {
                pc: GuestVa::new(CODE + 2)
            },
            executed: 1
        }
    );
}

#[test]
fn run_counts_instructions_up_to_the_limit() {
    let mut mem = space_with(&[0x1400_0000]); // b . (an endless loop)
    let mut state = State {
        pc: CODE,
        ..State::default()
    };

    let result = run(&mut state, &mut mem, 5).unwrap();

    assert_eq!(
        result,
        Run {
            exit: Exit::Limit,
            executed: 5
        }
    );
}

/// Runs `word` from `state` and returns X`n` after it.
fn x_after(word: u32, state: &State, n: usize) -> u64 {
    let (result, after) = step(word, state);
    assert_eq!(result.exit, Exit::Limit, "{word:#010x}");
    after.x[n]
}

#[test]
fn logical_immediates_decode_their_bitmask() {
    let x9 = with_x(&[(9, 0xf0)]);
    assert_eq!(x_after(0x927e_0d2a, &x9, 10), 0x30, "and x10, x9, #0x3c");
    let x10 = with_x(&[(10, 0x30)]);
    assert_eq!(x_after(0xb240_0d4b, &x10, 11), 0x3f, "orr x11, x10, #0xf");
    let all_ones = with_x(&[(0, u64::MAX)]);
    assert_eq!(
        x_after(0x3208_9fe0, &all_ones, 0),
        0xff00_ff00,
        "mov w0, #0xff00ff00 (orr w0, wzr) clears the upper half"
    );

    let (_, after) = step(0x927c_ec3f, &with_x(&[(1, 0x1234_5678_9abc_def7)]));
    assert_eq!(
        after.sp, 0x1234_5678_9abc_def0,
        "and sp, x1, #~0xf writes SP"
    );
}

#[test]
fn reserved_bitmask_immediates_are_undefined() {
    // and x0, x1 with N=0 and imms=0b111111 (no element size), then N=1 and imms all ones.
    for word in [0x9200_fc20, 0x9240_fc20] {
        let (result, _) = step(word, &State::default());
        assert_eq!(
            result.exit,
            Exit::Undefined {
                pc: GuestVa::new(CODE)
            },
            "{word:#010x}"
        );
    }
}

#[test]
fn ubfm_aliases_shift_and_extract() {
    let x12 = with_x(&[(12, 0xf000_0000_0000_00cf)]);
    assert_eq!(x_after(0xd37c_ed8d, &x12, 13), 0xcf0, "lsl x13, x12, #4");
    let x13 = with_x(&[(13, 0x8000_0000_0000_0cf0)]);
    assert_eq!(
        x_after(0xd348_fdae, &x13, 14),
        0x0080_0000_0000_000c,
        "lsr x14, x13, #8"
    );
    let x1 = with_x(&[(1, 0x1_8000_0000)]);
    assert_eq!(x_after(0x531f_7c20, &x1, 0), 1, "lsr w0, w1, #31");
    let x1 = with_x(&[(1, 0xabcd)]);
    assert_eq!(x_after(0xd348_2c20, &x1, 0), 0xb, "ubfx x0, x1, #8, #4");
}

#[test]
fn add_and_sub_shifted_register() {
    let x6 = with_x(&[(6, 23)]);
    assert_eq!(x_after(0x8b06_08c7, &x6, 7), 115, "add x7, x6, x6, lsl #2");
    let operands = with_x(&[(5, 123), (4, 100)]);
    assert_eq!(x_after(0xcb04_00a6, &operands, 6), 23, "sub x6, x5, x4");
    let operands = with_x(&[(5, 0), (4, 1)]);
    assert_eq!(x_after(0xcb04_00a6, &operands, 6), u64::MAX, "sub wraps");

    let operands = with_x(&[(1, 0x1000_0000), (2, 0x8000_0000)]);
    assert_eq!(
        x_after(0x0b82_1020, &operands, 0),
        0x0800_0000,
        "add w0, w1, w2, asr #4 shifts in the sign of w2"
    );
    let operands = with_x(&[(1, 0x1000), (2, 1 << 63)]);
    assert_eq!(
        x_after(0x8b82_1020, &operands, 0),
        0xf800_0000_0000_1000,
        "add x0, x1, x2, asr #4 shifts in the sign of x2"
    );
    let operands = with_x(&[(1, 10), (2, 1 << 63)]);
    assert_eq!(
        x_after(0xcb42_fc20, &operands, 0),
        9,
        "sub x0, x1, x2, lsr #63"
    );
}

#[test]
fn subs_register_sets_the_flags_at_its_width() {
    let subs = 0x6b02_0020; // subs w0, w1, w2
    let (_, after) = step(subs, &with_x(&[(1, 0xffff_ffff_0000_0001), (2, 2)]));
    assert_eq!((after.x[0], after.nzcv), (0xffff_ffff, N), "1 - 2 borrows");
    let (_, after) = step(subs, &with_x(&[(1, 5), (2, 5)]));
    assert_eq!((after.x[0], after.nzcv), (0, Z | C), "5 - 5");
}

#[test]
fn orr_orn_and_eor_shifted_register() {
    let x15 = with_x(&[(15, 0)]);
    assert_eq!(
        x_after(0x2a3f_03ef, &x15, 15),
        0xffff_ffff,
        "mvn w15, wzr (orn) clears the upper half"
    );
    let operands = with_x(&[(11, 0x3f), (9, 0xf0)]);
    assert_eq!(
        x_after(0xca09_016c, &operands, 12),
        0xcf,
        "eor x12, x11, x9"
    );
    let operands = with_x(&[(1, 1), (2, 0xff)]);
    assert_eq!(
        x_after(0xaac2_2020, &operands, 0),
        0xff00_0000_0000_0001,
        "orr x0, x1, x2, ror #8"
    );
}
