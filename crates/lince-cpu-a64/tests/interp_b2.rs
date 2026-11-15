// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Differential tests of the forms the B2 program (`tests/homebrew/b2.s`) adds to the Tier A
//! interpreter: conditional select, variable shift, MOVN, register-offset loads and stores,
//! the exclusives with the per-core monitor (ADR-0004), CLREX and `MRS TPIDRRO_EL0`.
//!
//! Encodings come from clang `--target=aarch64-none-elf` and `llvm-objdump -d`, except the
//! CONSTRAINED UNPREDICTABLE forms clang rejects, which change one register field of a listed
//! neighbour. Expected values follow the pseudocode of the Arm Architecture Reference Manual.

use lince_common::GuestVa;
use lince_cpu_a64::{Exclusive, Exit, Insn, State, decode, run};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};

const CODE: u64 = 0x1_0000;
const DATA: u64 = 0x2_0000;
const READ_ONLY: u64 = 0x3_0000;
const RW: Permission = Permission::READ.union(Permission::WRITE);
const RX: Permission = Permission::READ.union(Permission::EXECUTE);
const FLAG_Z: u32 = 1 << 30;

const LDXR_W0_X1: u32 = 0x885f_7c20;
const LDAXR_X0_X1: u32 = 0xc85f_fc20;
const STXR_W2_W0_X1: u32 = 0x8802_7c20;
const STLXR_W2_X0_X1: u32 = 0xc802_fc20;
const STLXR_W2_W0_X1: u32 = 0x8802_fc20;
const ADD_W0_W0_1: u32 = 0x1100_0400;
const CLREX: u32 = 0xd503_3f5f;

/// `words` at `CODE` (read-execute), a zeroed read-write page at `DATA` and a read-only page at
/// `READ_ONLY`.
fn space_with(words: &[u32]) -> AddressSpace {
    let mem = AddressSpace::new(SpaceConfig::for_host()).expect("space");
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
    mem.map(GuestVa::new(READ_ONLY), GUEST_PAGE_SIZE, Permission::READ)
        .expect("map read-only");
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

/// Runs the single instruction `word` on `state` and returns the exit.
fn exec_on(mem: &mut AddressSpace, word: u32, state: &mut State) -> Exit {
    mem.protect(GuestVa::new(CODE), GUEST_PAGE_SIZE, RW)
        .expect("unprotect code");
    mem.write(GuestVa::new(CODE), word).expect("write code");
    mem.protect(GuestVa::new(CODE), GUEST_PAGE_SIZE, RX)
        .expect("protect code");
    state.pc = CODE;
    run(state, mem, 1).expect("run").exit
}

/// Runs `word` on `state` in a fresh space and checks that it completed.
fn exec(word: u32, state: &mut State) {
    let mut mem = space_with(&[]);
    assert_eq!(exec_on(&mut mem, word, state), Exit::Limit, "{word:#010x}");
}

fn u32_at(mem: &AddressSpace, va: u64) -> u32 {
    mem.read(GuestVa::new(va)).expect("read data")
}

#[test]
fn csel_picks_rn_when_the_condition_holds_and_rm_otherwise() {
    let csel_w_eq = 0x1a82_0020; // csel w0, w1, w2, eq
    let csel_x_ne = 0x9a82_1020; // csel x0, x1, x2, ne
    let regs = [(1, 0xaaaa_aaaa_0000_0011), (2, 0xbbbb_bbbb_0000_0022)];

    let mut equal = with_x(&regs);
    equal.nzcv = FLAG_Z;
    exec(csel_w_eq, &mut equal);
    let mut different = with_x(&regs);
    exec(csel_w_eq, &mut different);
    let mut wide = with_x(&regs);
    exec(csel_x_ne, &mut wide);

    assert_eq!(equal.x[0], 0x11, "W result clears the upper half");
    assert_eq!(different.x[0], 0x22);
    assert_eq!(wide.x[0], 0xaaaa_aaaa_0000_0011);
}

#[test]
fn csinc_adds_one_to_rm_and_cset_writes_the_condition() {
    let csinc = 0x1a82_0420; // csinc w0, w1, w2, eq
    let cset = 0x1a9f_17e0; // cset w0, eq (csinc w0, wzr, wzr, ne)

    let mut not_equal = with_x(&[(1, 5), (2, 0xffff_ffff)]);
    exec(csinc, &mut not_equal);
    let mut set = with_x(&[(0, 7)]);
    set.nzcv = FLAG_Z;
    exec(cset, &mut set);
    let mut clear = with_x(&[(0, 7)]);
    exec(cset, &mut clear);

    assert_eq!(not_equal.x[0], 0, "Rm + 1 wraps at 32 bits");
    assert_eq!(set.x[0], 1);
    assert_eq!(clear.x[0], 0);
}

#[test]
fn lsrv_shifts_by_rm_modulo_the_width() {
    let lsr_w = 0x1ac2_2420; // lsr w0, w1, w2
    let lsr_x = 0x9ac2_2420; // lsr x0, x1, x2

    let mut narrow = with_x(&[(1, 0xffff_ffff_8000_0000), (2, 33)]);
    exec(lsr_w, &mut narrow);
    let mut wide = with_x(&[(1, 0x8000_0000_0000_0000), (2, 63 + 64)]);
    exec(lsr_x, &mut wide);

    assert_eq!(
        narrow.x[0], 0x4000_0000,
        "33 mod 32 = 1, upper half ignored"
    );
    assert_eq!(wide.x[0], 1);
}

#[test]
fn movn_writes_the_inverted_shifted_immediate() {
    let mut wide = with_x(&[]);
    exec(0x9280_0000, &mut wide); // mov x0, #-1
    let mut narrow = with_x(&[(0, u64::MAX)]);
    exec(0x12a0_0020, &mut narrow); // movn w0, #1, lsl #16

    assert_eq!(wide.x[0], u64::MAX);
    assert_eq!(narrow.x[0], 0xfffe_ffff);
}

#[test]
fn register_offset_loads_extend_and_scale_the_offset() {
    let mut mem = space_with(&[]);
    mem.write(GuestVa::new(DATA + 0x10), 0x1122_3344_5566_7788_u64)
        .expect("seed");
    let base = DATA + 0x20;

    let mut plain = with_x(&[(1, DATA), (2, 0x10)]);
    exec_on(&mut mem, 0xb862_6820, &mut plain); // ldr w0, [x1, x2]
    let mut scaled = with_x(&[(1, DATA), (2, 2)]);
    exec_on(&mut mem, 0xf862_7820, &mut scaled); // ldr x0, [x1, x2, lsl #3]
    let mut negative = with_x(&[(1, base), (2, 0xdead_beef_ffff_fffc)]);
    exec_on(&mut mem, 0xb862_d820, &mut negative); // ldr w0, [x1, w2, sxtw #2]
    let mut unsigned = with_x(&[(1, DATA), (2, 0xffff_ffff_0000_0010)]);
    exec_on(&mut mem, 0xf862_4820, &mut unsigned); // ldr x0, [x1, w2, uxtw]

    assert_eq!(plain.x[0], 0x5566_7788);
    assert_eq!(scaled.x[0], 0x1122_3344_5566_7788);
    assert_eq!(negative.x[0], 0x5566_7788, "-4 * 4 = -16 from DATA + 0x20");
    assert_eq!(
        unsigned.x[0], 0x1122_3344_5566_7788,
        "upper half of W2 ignored"
    );
}

#[test]
fn ldrb_and_strb_register_offset_move_one_byte() {
    let mut mem = space_with(&[]);
    let mut store = with_x(&[(0, 0x1ab), (1, DATA), (2, 5)]);
    let mut load = with_x(&[(0, u64::MAX), (1, DATA), (2, 5)]);

    exec_on(&mut mem, 0x3822_6820, &mut store); // strb w0, [x1, x2]
    exec_on(&mut mem, 0x3862_6820, &mut load); // ldrb w0, [x1, x2]

    assert_eq!(u32_at(&mem, DATA + 4), 0xab00);
    assert_eq!(load.x[0], 0xab);
}

#[test]
fn ldxr_reserves_and_a_matching_stxr_stores_reports_success_and_clears() {
    let mut mem = space_with(&[]);
    mem.write(GuestVa::new(DATA), 41_u32).expect("seed");
    let mut state = with_x(&[(1, DATA)]);

    exec_on(&mut mem, LDXR_W0_X1, &mut state);
    let reserved = state.exclusive;
    state.x[0] = 42;
    exec_on(&mut mem, STXR_W2_W0_X1, &mut state);

    assert_eq!(
        reserved,
        Some(Exclusive {
            address: GuestVa::new(DATA),
            value: 41,
            size: 4
        })
    );
    assert_eq!(state.x[2], 0, "status 0: stored");
    assert_eq!(u32_at(&mem, DATA), 42);
    assert_eq!(state.exclusive, None);
}

#[test]
fn ldaxr_and_stlxr_work_on_doublewords() {
    let mut mem = space_with(&[]);
    mem.write(GuestVa::new(DATA + 8), u64::MAX - 1)
        .expect("seed");
    let mut state = with_x(&[(1, DATA + 8)]);

    exec_on(&mut mem, LDAXR_X0_X1, &mut state);
    let loaded = state.x[0];
    state.x[0] = 0x0123_4567_89ab_cdef;
    exec_on(&mut mem, STLXR_W2_X0_X1, &mut state);

    assert_eq!(loaded, u64::MAX - 1);
    assert_eq!(state.x[2], 0);
    let stored: u64 = mem.read(GuestVa::new(DATA + 8)).expect("read");
    assert_eq!(stored, 0x0123_4567_89ab_cdef);
}

#[test]
fn stxr_fails_without_a_reservation_after_clrex_or_on_another_address() {
    let mut mem = space_with(&[]);
    let mut none = with_x(&[(0, 7), (1, DATA)]);
    let mut cleared = with_x(&[(1, DATA)]);
    let mut moved = with_x(&[(1, DATA)]);

    exec_on(&mut mem, STXR_W2_W0_X1, &mut none);
    exec_on(&mut mem, LDXR_W0_X1, &mut cleared);
    exec_on(&mut mem, CLREX, &mut cleared);
    cleared.x[0] = 7;
    exec_on(&mut mem, STXR_W2_W0_X1, &mut cleared);
    exec_on(&mut mem, LDXR_W0_X1, &mut moved);
    moved.x[0] = 7;
    moved.x[1] = DATA + 4;
    exec_on(&mut mem, STXR_W2_W0_X1, &mut moved);

    for state in [&none, &cleared, &moved] {
        assert_eq!(state.x[2], 1, "status 1: not stored");
        assert_eq!(state.exclusive, None);
    }
    assert_eq!(u32_at(&mem, DATA), 0);
    assert_eq!(u32_at(&mem, DATA + 4), 0);
}

#[test]
fn stxr_fails_when_memory_changed_since_the_load() {
    let mut mem = space_with(&[]);
    let mut state = with_x(&[(1, DATA)]);

    exec_on(&mut mem, LDXR_W0_X1, &mut state);
    mem.write(GuestVa::new(DATA), 9_u32).expect("other core");
    state.x[0] = 1;
    exec_on(&mut mem, STXR_W2_W0_X1, &mut state);

    assert_eq!(state.x[2], 1);
    assert_eq!(u32_at(&mem, DATA), 9);
}

/// Every interleaving of two cores running `ldxr; add #1; stlxr` on one word: when both load
/// before either stores, exactly one store-exclusive succeeds; in every order the word ends at
/// the number of successes, so no update is lost.
#[test]
fn two_cores_contending_on_an_exclusive_pair_never_both_succeed() {
    let program = [LDXR_W0_X1, ADD_W0_W0_1, STLXR_W2_W0_X1];
    let steps = program.len();
    let mut interleavings = 0;
    for order in 0u32..(1 << (2 * steps)) {
        if order.count_ones() as usize != steps {
            continue;
        }
        interleavings += 1;
        let mem = space_with(&program);
        let mut cores = [with_x(&[(1, DATA)]), with_x(&[(1, DATA)])];

        // The slot of each core's load-exclusive and store-exclusive.
        let mut loaded_at = [0; 2];
        let mut stored_at = [0; 2];
        for slot in 0..2 * steps {
            let which = usize::from((order >> slot) & 1 == 1);
            match cores[which].pc - CODE {
                0 => loaded_at[which] = slot,
                8 => stored_at[which] = slot,
                _ => {}
            }
            let result = run(&mut cores[which], &mem, 1).expect("run");
            assert_eq!(result.exit, Exit::Limit);
        }

        let successes = cores.iter().filter(|core| core.x[2] == 0).count();
        let overlapped = loaded_at[0] < stored_at[1] && loaded_at[1] < stored_at[0];
        if overlapped {
            assert_eq!(
                successes, 1,
                "order {order:#b}: one of two contending stores"
            );
        }
        assert_eq!(
            u32_at(&mem, DATA) as usize,
            successes,
            "order {order:#b}: no update lost"
        );
    }
    assert_eq!(interleavings, 20);
}

/// Runs `core` over `mem` in slices of `slice` instructions until it calls `SVC #0`.
fn run_to_svc(core: &mut State, mem: &AddressSpace, slice: u64) {
    let mut exit = Exit::Limit;
    while exit == Exit::Limit {
        exit = run(core, mem, slice).expect("run").exit;
    }
    assert_eq!(exit, Exit::Svc { number: 0 });
}

/// Two host threads share one address space and each increments one word `ROUNDS` times with
/// an `ldxr; add; stlxr` retry loop through [`run`], as two guest cores would (ADR-0004,
/// ADR-0019 B2 scope): the word ends at twice `ROUNDS`, so no update is lost.
#[test]
fn two_threads_contending_on_an_exclusive_pair_lose_no_update() {
    const ROUNDS: u64 = 50_000;
    const SLICE: u64 = 1000;
    let program = [
        LDXR_W0_X1,
        ADD_W0_W0_1,
        STLXR_W2_W0_X1,
        0x35ff_ffa2, // cbnz w2, <ldxr>: retry when the store-exclusive failed
        0xd100_0463, // sub x3, x3, #1
        0xb5ff_ff63, // cbnz x3, <ldxr>
        0xd400_0001, // svc #0
    ];
    let mem = space_with(&program);
    let start = std::sync::Barrier::new(2);

    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                let mut core = with_x(&[(1, DATA), (3, ROUNDS)]);
                start.wait();
                run_to_svc(&mut core, &mem, SLICE);
            });
        }
    });

    assert_eq!(u64::from(u32_at(&mem, DATA)), 2 * ROUNDS);
}

#[test]
fn misaligned_exclusives_abort_and_change_nothing() {
    let mut mem = space_with(&[]);
    let mut load = with_x(&[(1, DATA + 2)]);
    let mut store = with_x(&[(1, DATA + 2)]);
    store.exclusive = Some(Exclusive {
        address: GuestVa::new(DATA + 2),
        value: 0,
        size: 4,
    });

    let load_exit = exec_on(&mut mem, LDXR_W0_X1, &mut load);
    let store_exit = exec_on(&mut mem, STXR_W2_W0_X1, &mut store);

    let abort = |write| Exit::DataAbort {
        pc: GuestVa::new(CODE),
        address: GuestVa::new(DATA + 2),
        write,
    };
    assert_eq!(load_exit, abort(false));
    assert_eq!(store_exit, abort(true));
    assert_eq!(load.exclusive, None);
    assert!(
        store.exclusive.is_some(),
        "an aborted instruction changes nothing"
    );
}

#[test]
fn stxr_to_read_only_memory_aborts_and_keeps_the_reservation() {
    let mut mem = space_with(&[]);
    let mut state = with_x(&[(0, 5), (1, READ_ONLY)]);

    exec_on(&mut mem, LDXR_W0_X1, &mut state);
    let exit = exec_on(&mut mem, STXR_W2_W0_X1, &mut state);

    assert_eq!(
        exit,
        Exit::DataAbort {
            pc: GuestVa::new(CODE),
            address: GuestVa::new(READ_ONLY),
            write: true,
        }
    );
    assert!(state.exclusive.is_some());
    assert_eq!(u32_at(&mem, READ_ONLY), 0);
}

#[test]
fn stxr_with_rs_overlapping_rt_or_rn_is_undefined() {
    let status_on_value = 0x8800_fc20; // stlxr w0, w0, [x1]: Rs of STLXR_W2_W0_X1 set to 0
    let status_on_base = 0x8801_fc20; // stlxr w1, w0, [x1]: Rs set to 1
    let mut mem = space_with(&[]);

    for word in [status_on_value, status_on_base] {
        let mut state = with_x(&[(1, DATA)]);
        assert_eq!(
            exec_on(&mut mem, word, &mut state),
            Exit::Undefined {
                pc: GuestVa::new(CODE)
            }
        );
    }
}

#[test]
fn mrs_reads_tpidrro_el0_and_leaves_other_registers_unimplemented() {
    let mut mem = space_with(&[]);
    let mut tls = with_x(&[]);
    tls.tpidrro_el0 = 0x0123_4000;
    let mut other = with_x(&[]);

    exec_on(&mut mem, 0xd53b_d060, &mut tls); // mrs x0, tpidrro_el0
    let exit = exec_on(&mut mem, 0xd53b_d040, &mut other); // mrs x0, tpidr_el0

    assert_eq!(tls.x[0], 0x0123_4000);
    assert_eq!(
        exit,
        Exit::Unimplemented {
            pc: GuestVa::new(CODE),
            insn: decode(0xd53b_d040).expect("MRS decodes"),
        }
    );
    assert_eq!(decode(0xd53b_d040).map(Insn::name).as_deref(), Some("MRS"));
}
