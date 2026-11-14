// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Differential tests of the Tier A interpreter's loads and stores against the Arm
//! specification, on both memory paths (ADR-0031): inline through the host base when one
//! region holds the access, and the slow path otherwise.
//!
//! Encodings come from clang `--target=aarch64-none-elf` and `llvm-objdump -d`, except the
//! CONSTRAINED UNPREDICTABLE forms clang rejects, which are derived from a listed neighbour by
//! changing one register field. Expected values are worked out from the instruction's
//! pseudocode in the Arm Architecture Reference Manual.

use lince_common::GuestVa;
use lince_cpu_a64::{Exit, Run, State, run};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};

const CODE: u64 = 0x1_0000;
const DATA: u64 = 0x2_0000;
const UNMAPPED: u64 = 0x5_0000;
const RW: Permission = Permission::READ.union(Permission::WRITE);
const RX: Permission = Permission::READ.union(Permission::EXECUTE);

/// `words` at `CODE` (read-execute) and a zeroed read-write page at `DATA`.
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
    mem.map(GuestVa::new(DATA), GUEST_PAGE_SIZE, RW)
        .expect("map data");
    mem
}

fn with_x(pairs: &[(usize, u64)]) -> State {
    let mut state = State {
        pc: CODE,
        ..State::default()
    };
    for &(n, value) in pairs {
        state.x[n] = value;
    }
    state
}

/// Runs the first instruction at `CODE` once and returns the run.
fn step(mem: &mut AddressSpace, state: &mut State) -> Run {
    run(state, mem, 1).expect("run")
}

/// Runs `word` and checks that it completed.
fn exec(mem: &mut AddressSpace, word: u32, state: &mut State) {
    mem.protect(GuestVa::new(CODE), GUEST_PAGE_SIZE, RW)
        .expect("unprotect code");
    mem.write(GuestVa::new(CODE), word).expect("write code");
    mem.protect(GuestVa::new(CODE), GUEST_PAGE_SIZE, RX)
        .expect("protect code");
    state.pc = CODE;
    let result = step(mem, state);
    assert_eq!(result.exit, Exit::Limit, "{word:#010x}");
    assert_eq!(state.pc, CODE + 4);
}

fn u64_at(mem: &AddressSpace, va: u64) -> u64 {
    mem.read(GuestVa::new(va)).expect("read data")
}

fn u32_at(mem: &AddressSpace, va: u64) -> u32 {
    mem.read(GuestVa::new(va)).expect("read data")
}

#[test]
fn ldr_literal_reads_x_or_zero_extended_w_relative_to_pc() {
    let nop = 0xd503_201f;
    let mut mem = space_with(&[0x5800_0040, nop, 0x5566_7788, 0x1122_3344]); // ldr x0, #8
    let mut state = with_x(&[]);
    step(&mut mem, &mut state);
    assert_eq!(state.x[0], 0x1122_3344_5566_7788);

    let mut mem = space_with(&[0x1800_0040, nop, 0x5566_7788, 0x1122_3344]); // ldr w0, #8
    let mut state = with_x(&[(0, u64::MAX)]);
    step(&mut mem, &mut state);
    assert_eq!(state.x[0], 0x5566_7788);
}

#[test]
fn unsigned_offset_loads_scale_the_offset_and_zero_extend() {
    let mut mem = space_with(&[]);
    mem.write(GuestVa::new(DATA), 0x1122_3344_5566_7788_u64)
        .unwrap();
    mem.write(GuestVa::new(DATA + 8), 0x99aa_bbcc_u32).unwrap();
    mem.write(GuestVa::new(DATA + 0xff8), 0x0123_4567_89ab_cdef_u64)
        .unwrap();
    let cases = [
        (0xf940_0020, 0x1122_3344_5566_7788, "ldr x0, [x1]"),
        (0xb940_0820, 0x99aa_bbcc, "ldr w0, [x1, #8]"),
        (0x3940_0420, 0x77, "ldrb w0, [x1, #1]"),
        (0x7940_0420, 0x5566, "ldrh w0, [x1, #2]"),
        (0xf947_fc20, 0x0123_4567_89ab_cdef, "ldr x0, [x1, #0xff8]"),
    ];
    for (word, expected, text) in cases {
        let mut state = with_x(&[(0, u64::MAX), (1, DATA)]);
        exec(&mut mem, word, &mut state);
        assert_eq!(state.x[0], expected, "{text}");
    }
}

#[test]
fn unsigned_offset_stores_write_the_low_bytes_and_take_sp_as_base() {
    let mut mem = space_with(&[]);
    let mut state = with_x(&[(0, 0xdead_beef_0123_4567)]);
    state.sp = DATA;
    exec(&mut mem, 0xf900_0be0, &mut state); // str x0, [sp, #16]
    assert_eq!(u64_at(&mem, DATA + 16), 0xdead_beef_0123_4567);

    let mut state = with_x(&[(0, 0xffff_ffff_aabb_ccdd), (1, DATA)]);
    exec(&mut mem, 0xb900_0420, &mut state); // str w0, [x1, #4]
    assert_eq!(u32_at(&mem, DATA + 4), 0xaabb_ccdd);
    assert_eq!(u32_at(&mem, DATA + 8), 0, "a W store writes four bytes");
}

#[test]
fn pre_and_post_index_write_the_base_back() {
    let mut mem = space_with(&[]);
    let mut state = with_x(&[(0, 0x1ab), (1, DATA)]);
    exec(&mut mem, 0x3801_0c20, &mut state); // strb w0, [x1, #16]!
    assert_eq!(mem.read::<u8>(GuestVa::new(DATA + 16)).unwrap(), 0xab);
    assert_eq!(state.x[1], DATA + 16);

    mem.write(GuestVa::new(DATA + 16), 0x5a_u8).unwrap();
    let mut state = with_x(&[(0, u64::MAX), (1, DATA + 16)]);
    exec(&mut mem, 0x385f_0420, &mut state); // ldrb w0, [x1], #-16
    assert_eq!((state.x[0], state.x[1]), (0x5a, DATA));
}

#[test]
fn pairs_store_and_load_two_registers() {
    let mut mem = space_with(&[]);
    let mut state = with_x(&[(1, DATA + 32), (2, 0x1111), (3, 0x2222)]);
    exec(&mut mem, 0xa93f_0c22, &mut state); // stp x2, x3, [x1, #-16]
    assert_eq!(
        (u64_at(&mem, DATA + 16), u64_at(&mem, DATA + 24)),
        (0x1111, 0x2222)
    );
    exec(&mut mem, 0xa97f_1424, &mut state); // ldp x4, x5, [x1, #-16]
    assert_eq!((state.x[4], state.x[5]), (0x1111, 0x2222));
    assert_eq!(
        state.x[1],
        DATA + 32,
        "the signed-offset forms do not write back"
    );

    let mut state = with_x(&[(1, DATA), (2, 0xa_0000_0001), (3, 0xb_0000_0002)]);
    exec(&mut mem, 0x2901_0c22, &mut state); // stp w2, w3, [x1, #8]
    assert_eq!((u32_at(&mem, DATA + 8), u32_at(&mem, DATA + 12)), (1, 2));
    state.x[4] = u64::MAX;
    state.x[5] = u64::MAX;
    exec(&mut mem, 0x2941_1424, &mut state); // ldp w4, w5, [x1, #8]
    assert_eq!((state.x[4], state.x[5]), (1, 2));
}

#[test]
fn constrained_unpredictable_forms_are_undefined_and_change_nothing() {
    let cases = [
        (0x3801_0c21, "strb w1, [x1, #16]!"),
        (0x385f_0421, "ldrb w1, [x1], #-16"),
        (0xa97f_1024, "ldp x4, x4, [x1, #-16]"),
    ];
    for (word, text) in cases {
        let mut mem = space_with(&[word]);
        let start = with_x(&[(1, DATA + 16), (4, 7)]);
        let mut state = start.clone();
        let result = step(&mut mem, &mut state);
        assert_eq!(
            result.exit,
            Exit::Undefined {
                pc: GuestVa::new(CODE)
            },
            "{text}"
        );
        assert_eq!(state, start, "{text}");
    }
}

#[test]
fn an_inaccessible_address_is_a_data_abort_that_changes_nothing() {
    let cases = [
        (
            0xf940_0020,
            UNMAPPED,
            false,
            "ldr x0, [x1] from an unmapped page",
        ),
        (
            0xf900_0020,
            UNMAPPED,
            true,
            "str x0, [x1] to an unmapped page",
        ),
        (0xf900_0020, CODE, true, "str x0, [x1] to read-execute code"),
    ];
    for (word, address, write, text) in cases {
        let mut mem = space_with(&[word]);
        let start = with_x(&[(0, 0x1234), (1, address)]);
        let mut state = start.clone();
        let result = step(&mut mem, &mut state);
        assert_eq!(
            result,
            Run {
                exit: Exit::DataAbort {
                    pc: GuestVa::new(CODE),
                    address: GuestVa::new(address),
                    write
                },
                executed: 0
            },
            "{text}"
        );
        assert_eq!(state, start, "{text}");
        assert_eq!(u32_at(&mem, CODE), word, "{text}");
    }
}

#[test]
fn a_pair_that_runs_off_mapped_memory_writes_neither_half() {
    let last = DATA + GUEST_PAGE_SIZE - 8;
    let mut mem = space_with(&[0xa93f_0c22]); // stp x2, x3, [x1, #-16]
    let mut state = with_x(&[(1, last + 16), (2, 0x1111), (3, 0x2222)]);
    let result = step(&mut mem, &mut state);
    assert_eq!(
        result.exit,
        Exit::DataAbort {
            pc: GuestVa::new(CODE),
            address: GuestVa::new(last),
            write: true
        }
    );
    assert_eq!(u64_at(&mem, last), 0);
}

#[test]
fn an_access_across_two_regions_takes_the_slow_path() {
    let mut mem = space_with(&[]);
    let next = DATA + GUEST_PAGE_SIZE;
    mem.map(GuestVa::new(next), GUEST_PAGE_SIZE, RW).unwrap();
    assert_eq!(
        mem.regions().count(),
        3,
        "the data pages are two regions, so no single region holds the access"
    );
    let straddle = next - 4;
    let mut state = with_x(&[(0, 0x0807_0605_0403_0201), (1, straddle)]);
    exec(&mut mem, 0xf900_0020, &mut state); // str x0, [x1]
    assert_eq!(u64_at(&mem, straddle), 0x0807_0605_0403_0201);

    let mut state = with_x(&[(1, straddle)]);
    exec(&mut mem, 0xf940_0020, &mut state); // ldr x0, [x1]
    assert_eq!(state.x[0], 0x0807_0605_0403_0201);
}

#[test]
fn the_slow_path_checks_permissions_too() {
    let mut mem = space_with(&[0xf900_0020]); // str x0, [x1]
    let next = DATA + GUEST_PAGE_SIZE;
    mem.map(GuestVa::new(next), GUEST_PAGE_SIZE, Permission::READ)
        .unwrap();
    let straddle = next - 4;
    let mut state = with_x(&[(0, u64::MAX), (1, straddle)]);
    let result = step(&mut mem, &mut state);
    assert_eq!(
        result.exit,
        Exit::DataAbort {
            pc: GuestVa::new(CODE),
            address: GuestVa::new(straddle),
            write: true
        }
    );
    assert_eq!(u64_at(&mem, straddle), 0, "neither page was written");
}
