// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The interrupt flag of `run_interruptible` (ADR-0032 §2): the interpreter returns
//! `Exit::Interrupted` at a safe point (entry or the end of a block), never inside a block.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use lince_common::GuestVa;
use lince_cpu_a64::{Exit, State, run, run_interruptible};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};

const CODE: u64 = 0x1_0000;
const RX: Permission = Permission::READ.union(Permission::EXECUTE);
const ADD_X0_X0_1: u32 = 0x9100_0400;
/// Straight-line instructions in the loop body before its backward branch.
const BODY: u32 = 15;

/// `BODY` times `add x0, x0, #1`, then `b` back to the first one: one block, looping forever.
fn spin_loop() -> AddressSpace {
    let mem = AddressSpace::new(SpaceConfig::for_host()).expect("space");
    let code = GuestVa::new(CODE);
    mem.map(code, GUEST_PAGE_SIZE, Permission::READ | Permission::WRITE)
        .expect("map code");
    let mut words = vec![ADD_X0_X0_1; usize::try_from(BODY).expect("body length")];
    // `b #-4*BODY`: imm26 is the offset in instructions, two's complement.
    words.push(0x1400_0000 | (BODY.wrapping_neg() & 0x03FF_FFFF));
    for (i, word) in (0u64..).zip(&words) {
        mem.write(code.wrapping_add(4 * i), *word)
            .expect("write code");
    }
    mem.protect(code, GUEST_PAGE_SIZE, RX)
        .expect("protect code");
    mem
}

fn at_code() -> State {
    State {
        pc: CODE,
        ..State::default()
    }
}

#[test]
fn a_flag_set_before_the_call_stops_the_core_before_the_first_instruction() {
    let mem = spin_loop();
    let mut state = at_code();
    let flag = AtomicBool::new(true);
    let result = run_interruptible(&mut state, &mem, 1_000, &flag).expect("run");
    assert_eq!(result.exit, Exit::Interrupted);
    assert_eq!(result.executed, 0);
    assert_eq!(state, at_code(), "nothing ran");
    assert!(
        flag.load(Ordering::Relaxed),
        "the interpreter never clears the flag"
    );
}

#[test]
fn a_clear_flag_runs_exactly_like_run() {
    let mem = spin_loop();
    let flag = AtomicBool::new(false);
    let mut interruptible = at_code();
    let mut plain = at_code();
    let a = run_interruptible(&mut interruptible, &mem, 100, &flag).expect("run");
    let b = run(&mut plain, &mem, 100).expect("run");
    assert_eq!(a, b);
    assert_eq!(a.exit, Exit::Limit);
    assert_eq!(interruptible, plain);
}

#[test]
fn a_flag_set_by_another_thread_stops_a_spinning_core_at_a_block_boundary() {
    let mem = spin_loop();
    let mut state = at_code();
    let flag = AtomicBool::new(false);
    let result = thread::scope(|scope| {
        scope.spawn(|| {
            thread::sleep(Duration::from_millis(5));
            flag.store(true, Ordering::Release);
        });
        run_interruptible(&mut state, &mem, u64::MAX, &flag).expect("run")
    });
    assert_eq!(result.exit, Exit::Interrupted);
    let block = u64::from(BODY) + 1;
    assert!(result.executed > 0, "the core ran before the flag was set");
    assert_eq!(result.executed % block, 0, "whole blocks only");
    assert_eq!(state.pc, CODE, "stopped at the start of the block");
    assert_eq!(state.x[0], result.executed / block * u64::from(BODY));
}
