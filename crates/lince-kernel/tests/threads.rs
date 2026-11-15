// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest threads on the six core threads (T-051): small guest programs create, start, move,
//! reprioritize and end threads through the thread SVCs, and record what they saw in memory.
//!
//! Encodings follow the A64 base instruction formats of the Arm Architecture Reference Manual.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use lince_common::GuestVa;
use lince_kernel::{
    CURRENT_THREAD, DeferredReply, Finish, IPC_MESSAGE_SIZE, IpcMessage, Kernel, KernelError, Regs,
    RunError, SessionObject, Sessions, TLS_BASE, TLS_SIZE, ThreadParams, Ticker, UnknownCommand,
    run_cores, run_cores_with,
};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};
use lince_trace::{EventKind, NameId, ThreadRole, Trace};

const CODE: u64 = 0x1_0000;
const DATA: u64 = 0x4_0000;
const STACK: u64 = 0x5_0000;
/// Each guest function lives `FUNCTION` bytes after the previous one.
const FUNCTION: u64 = 0x400;
const PRIORITY: u32 = 44;

const CREATE_THREAD: u16 = 0x08;
const START_THREAD: u16 = 0x09;
const EXIT_THREAD: u16 = 0x0A;
const SLEEP_THREAD: u16 = 0x0B;
const GET_THREAD_PRIORITY: u16 = 0x0C;
const SET_THREAD_PRIORITY: u16 = 0x0D;
const GET_THREAD_CORE_MASK: u16 = 0x0E;
const SET_THREAD_CORE_MASK: u16 = 0x0F;
const GET_CURRENT_PROCESSOR_NUMBER: u16 = 0x10;
const EXIT_PROCESS: u16 = 0x07;
const SIGNAL_EVENT: u16 = 0x11;
const WAIT_SYNCHRONIZATION: u16 = 0x18;
const CANCEL_SYNCHRONIZATION: u16 = 0x19;
const CREATE_EVENT: u16 = 0x45;
const ARBITRATE_LOCK: u16 = 0x1A;
const ARBITRATE_UNLOCK: u16 = 0x1B;
const WAIT_KEY: u16 = 0x1C;
const SIGNAL_KEY: u16 = 0x1D;
/// The mutex-word bit that says threads wait (tests/homebrew/b2.s).
const WAITERS: u64 = 0x4000_0000;
/// Where arbiter programs keep their mutex word and condition-variable key.
const MUTEX: u32 = 0x50;
const KEY: u32 = 0x58;
const TIMED_OUT: u64 = 0xEA01;
const CANCELLED: u64 = 0xEC01;
/// Where wait programs keep their handle arrays.
const HANDLES: u32 = 0x80;

/// A tiny A64 encoder for the instructions these programs use.
#[derive(Default)]
struct Asm(Vec<u32>);

impl Asm {
    fn emit(&mut self, word: u32) -> &mut Self {
        self.0.push(word);
        self
    }

    /// `MOVZ`/`MOVK` sequence that loads the 64-bit `value` into `Xd`.
    fn mov(&mut self, d: u32, value: u64) -> &mut Self {
        for hw in 0..4u32 {
            let imm = u32::try_from((value >> (16 * hw)) & 0xFFFF).expect("test setup");
            let base = if hw == 0 { 0xD280_0000 } else { 0xF280_0000 };
            self.emit(base | hw << 21 | imm << 5 | d);
        }
        self
    }

    /// `MOV Xd, Xm` (`ORR Xd, XZR, Xm`).
    fn mov_reg(&mut self, d: u32, m: u32) -> &mut Self {
        self.emit(0xAA00_03E0 | m << 16 | d)
    }

    fn orr(&mut self, d: u32, n: u32, m: u32) -> &mut Self {
        self.emit(0xAA00_0000 | m << 16 | n << 5 | d)
    }

    fn sub_imm(&mut self, d: u32, n: u32, imm: u32) -> &mut Self {
        self.emit(0xD100_0000 | imm << 10 | n << 5 | d)
    }

    /// `ADD Xd, Xn, Xm, LSL #3`.
    fn add_lsl3(&mut self, d: u32, n: u32, m: u32) -> &mut Self {
        self.emit(0x8B00_0000 | m << 16 | 3 << 10 | n << 5 | d)
    }

    fn add_imm(&mut self, d: u32, n: u32, imm: u32) -> &mut Self {
        self.emit(0x9100_0000 | imm << 10 | n << 5 | d)
    }

    /// `STR Xt, [Xn, #offset]`.
    fn str(&mut self, t: u32, n: u32, offset: u32) -> &mut Self {
        self.emit(0xF900_0000 | (offset / 8) << 10 | n << 5 | t)
    }

    /// `STR Wt, [Xn, #offset]`.
    fn str_w(&mut self, t: u32, n: u32, offset: u32) -> &mut Self {
        self.emit(0xB900_0000 | (offset / 4) << 10 | n << 5 | t)
    }

    /// `WaitSynchronization` on the `count` handles at `[Xn, #HANDLES]` with `timeout_ns`
    /// (negative: forever); clobbers `X0`–`X3`.
    fn wait(&mut self, n: u32, count: u64, timeout_ns: i64) -> &mut Self {
        self.add_imm(1, n, HANDLES)
            .mov(2, count)
            .mov(3, timeout_ns.cast_unsigned())
            .svc(WAIT_SYNCHRONIZATION)
    }

    /// Appends `Xv` to the list at `[Xn]`: count at offset 0, entries after it; clobbers
    /// `X10`–`X13`.
    fn append(&mut self, n: u32, v: u32) -> &mut Self {
        self.ldr(11, n, 0)
            .add_imm(12, 11, 1)
            .str(12, n, 0)
            .add_lsl3(13, n, 11)
            .str(v, 13, 8)
    }

    /// `LDR Wt, [Xn, #offset]`.
    fn ldr_w(&mut self, t: u32, n: u32, offset: u32) -> &mut Self {
        self.emit(0xB940_0000 | (offset / 4) << 10 | n << 5 | t)
    }

    /// `ArbitrateLock(owner = Xo, [Xn, #MUTEX], tag = Xt)`; clobbers `X0`–`X2`.
    fn lock(&mut self, n: u32, owner: u32, tag: u32) -> &mut Self {
        self.mov_reg(0, owner)
            .add_imm(1, n, MUTEX)
            .mov_reg(2, tag)
            .svc(ARBITRATE_LOCK)
    }

    /// `ArbitrateUnlock([Xn, #MUTEX])`; clobbers `X0`.
    fn unlock(&mut self, n: u32) -> &mut Self {
        self.add_imm(0, n, MUTEX).svc(ARBITRATE_UNLOCK)
    }

    /// `WaitProcessWideKeyAtomic([Xn, #KEY], [Xn, #MUTEX], tag = Xt, timeout_ns)`; clobbers
    /// `X0`–`X3`.
    fn wait_key(&mut self, n: u32, tag: u32, timeout_ns: i64) -> &mut Self {
        self.add_imm(0, n, KEY)
            .add_imm(1, n, MUTEX)
            .mov_reg(2, tag)
            .mov(3, timeout_ns.cast_unsigned())
            .svc(WAIT_KEY)
    }

    /// `SignalProcessWideKey([Xn, #KEY], count)`; clobbers `X0`–`X1`.
    fn signal_key(&mut self, n: u32, count: i64) -> &mut Self {
        self.add_imm(0, n, KEY)
            .mov(1, count.cast_unsigned())
            .svc(SIGNAL_KEY)
    }

    /// `LDR Xt, [Xn, #offset]`.
    fn ldr(&mut self, t: u32, n: u32, offset: u32) -> &mut Self {
        self.emit(0xF940_0000 | (offset / 8) << 10 | n << 5 | t)
    }

    fn mrs_tpidrro(&mut self, t: u32) -> &mut Self {
        self.emit(0xD53B_D060 | t)
    }

    fn svc(&mut self, number: u16) -> &mut Self {
        self.emit(0xD400_0001 | u32::from(number) << 5)
    }

    /// `CBNZ Xt` back to the instruction at index `target`.
    fn cbnz_back(&mut self, t: u32, target: usize) -> &mut Self {
        let back = i32::try_from(self.0.len() - target).expect("test setup");
        let imm = (-back).cast_unsigned() & 0x7_FFFF;
        self.emit(0xB500_0000 | imm << 5 | t)
    }

    /// `B` back to the instruction at index `target`.
    fn b_back(&mut self, target: usize) -> &mut Self {
        let back = i32::try_from(self.0.len() - target).expect("test setup");
        self.emit(0x1400_0000 | ((-back).cast_unsigned() & 0x03FF_FFFF))
    }

    /// Loops until the word at `[Xn, #offset]` equals `value`; clobbers `X12`.
    fn wait_for(&mut self, n: u32, offset: u32, value: u32) -> &mut Self {
        let top = self.here();
        self.ldr(12, n, offset)
            .sub_imm(12, 12, value)
            .cbnz_back(12, top)
    }

    /// `B .`: spins forever.
    fn spin(&mut self) -> &mut Self {
        self.emit(0x1400_0000)
    }

    fn here(&self) -> usize {
        self.0.len()
    }

    /// Creates a thread at `entry` with `arg`, priority and core, then starts it.
    fn spawn(&mut self, entry: u64, arg: u64, priority: u32, core: u32) -> &mut Self {
        self.mov(1, entry)
            .mov(2, arg)
            .mov(3, STACK)
            .mov(4, u64::from(priority))
            .mov(5, u64::from(core))
            .svc(CREATE_THREAD)
            .mov_reg(0, 1)
            .svc(START_THREAD)
    }
}

/// The address of guest function `n`.
fn function(n: u64) -> u64 {
    CODE + n * FUNCTION
}

/// A space with each function of `functions` at [`function`] `n`, and zeroed data and stack.
fn space_with(functions: &[&Asm]) -> AddressSpace {
    let rw = Permission::READ | Permission::WRITE;
    let mem = AddressSpace::new(SpaceConfig::for_host()).expect("test setup");
    let code_size = GUEST_PAGE_SIZE * 2;
    mem.map(GuestVa::new(CODE), code_size, rw)
        .expect("test setup");
    for (n, asm) in (0u64..).zip(functions) {
        assert!(
            4 * u64::try_from(asm.0.len()).expect("test setup") <= FUNCTION,
            "function {n} too long"
        );
        for (i, word) in (0u64..).zip(&asm.0) {
            mem.write(GuestVa::new(function(n) + 4 * i), *word)
                .expect("test setup");
        }
    }
    mem.protect(
        GuestVa::new(CODE),
        code_size,
        Permission::READ | Permission::EXECUTE,
    )
    .expect("test setup");
    mem.map(GuestVa::new(DATA), GUEST_PAGE_SIZE, rw)
        .expect("test setup");
    mem.map(GuestVa::new(STACK - GUEST_PAGE_SIZE), GUEST_PAGE_SIZE, rw)
        .expect("test setup");
    mem
}

fn kernel() -> Kernel {
    Kernel::new(&mut Trace::new(64)).expect("test setup")
}

fn main_thread(arg: u64) -> ThreadParams {
    ThreadParams {
        entry: GuestVa::new(function(0)),
        arg,
        stack_top: STACK,
        priority: PRIORITY,
        core: 0,
    }
}

fn data(mem: &AddressSpace, offset: u64) -> u64 {
    mem.read::<u64>(GuestVa::new(DATA + offset))
        .expect("test setup")
}

/// Runs `functions` with function 0 as the main thread on core 0.
fn run(functions: &[&Asm]) -> (AddressSpace, Result<Finish, RunError>) {
    let mem = space_with(functions);
    let mut kernel = kernel();
    kernel
        .spawn_thread(&mem, main_thread(DATA))
        .expect("test setup");
    let result = run_cores(&mut kernel, &mem);
    (mem, result)
}

/// A worker that stores its core number at its argument and exits.
fn report_core() -> Asm {
    let mut asm = Asm::default();
    asm.mov_reg(9, 0)
        .svc(GET_CURRENT_PROCESSOR_NUMBER)
        .str(0, 9, 0)
        .svc(EXIT_THREAD);
    asm
}

#[test]
fn threads_run_on_the_core_they_were_created_for() {
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .svc(GET_CURRENT_PROCESSOR_NUMBER)
        .add_imm(0, 0, 0x10)
        .str(0, 9, 0);
    for core in 1..6 {
        main.spawn(function(1), DATA + 8 * u64::from(core), PRIORITY, core);
    }
    main.svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &report_core()]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!(data(&mem, 0), 0x10, "main on core 0");
    for core in 1..6 {
        assert_eq!(data(&mem, 8 * core), core);
    }
}

/// A thread loops across many slices, ORs `core - expected` into an accumulator, moves itself
/// with `SetThreadCoreMask`, loops again and reads its mask back. A zero accumulator means it
/// never saw another core.
#[test]
fn a_pinned_thread_never_runs_elsewhere_and_moves_only_when_told() {
    const ROUNDS: u64 = 20_000;
    let mut worker = Asm::default();
    worker.mov_reg(9, 0).mov(10, 0).mov(13, 0);
    for (acc, core) in [(10, 2), (13, 4)] {
        worker.mov(11, ROUNDS);
        let top = worker.here();
        worker
            .svc(GET_CURRENT_PROCESSOR_NUMBER)
            .sub_imm(12, 0, core)
            .orr(acc, acc, 12)
            .sub_imm(11, 11, 1)
            .cbnz_back(11, top);
        if acc == 10 {
            worker
                .mov(0, u64::from(CURRENT_THREAD))
                .mov(1, 4)
                .mov(2, 0b1_0000)
                .svc(SET_THREAD_CORE_MASK);
        }
    }
    worker
        .str(0, 9, 32)
        .mov(2, u64::from(CURRENT_THREAD))
        .svc(GET_THREAD_CORE_MASK)
        .str(10, 9, 0)
        .str(13, 9, 8)
        .str(1, 9, 16)
        .str(2, 9, 24)
        .svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.spawn(function(1), DATA, PRIORITY, 2)
        .spawn(function(2), DATA + 0x100, PRIORITY, 2)
        .svc(EXIT_THREAD);
    let mut busy = Asm::default();
    busy.mov(11, ROUNDS);
    let top = busy.here();
    busy.sub_imm(11, 11, 1).cbnz_back(11, top).svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &worker, &busy]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!(data(&mem, 0), 0, "only core 2 before the move");
    assert_eq!(data(&mem, 8), 0, "only core 4 after the move");
    assert_eq!(data(&mem, 32), 4, "the last core seen");
    assert_eq!((data(&mem, 16), data(&mem, 24)), (4, 0b1_0000));
}

/// Two workers on the main thread's core, started low priority first by a main thread that
/// outranks both: the high-priority one runs first once the main thread exits. Each appends
/// its argument to a list at `DATA`. (Under T-051 the main thread kept priority 44; with
/// prompt preemption the priority-40 worker would then run as soon as it started.)
#[test]
fn a_core_runs_its_highest_priority_ready_thread_first() {
    let mut worker = Asm::default();
    worker
        .mov(10, DATA)
        .ldr(11, 10, 0)
        .add_imm(12, 11, 1)
        .str(12, 10, 0)
        .add_lsl3(13, 10, 11)
        .str(0, 13, 8)
        .svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov(0, u64::from(CURRENT_THREAD))
        .mov(1, 10)
        .svc(SET_THREAD_PRIORITY)
        .spawn(function(1), 0xA, 40, 0)
        .spawn(function(1), 0xB, 20, 0)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &worker]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!(
        (data(&mem, 0), data(&mem, 8), data(&mem, 16)),
        (2, 0xB, 0xA)
    );
}

#[test]
fn set_thread_priority_is_read_back_by_get_thread_priority() {
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .mov(0, u64::from(CURRENT_THREAD))
        .mov(1, 10)
        .svc(SET_THREAD_PRIORITY)
        .str(0, 9, 0)
        .mov(1, u64::from(CURRENT_THREAD))
        .svc(GET_THREAD_PRIORITY)
        .str(0, 9, 8)
        .str(1, 9, 16)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!((data(&mem, 0), data(&mem, 8), data(&mem, 16)), (0, 0, 10));
}

/// Every thread gets its own writable TLS block, which `TPIDRRO_EL0` points to.
#[test]
fn each_thread_has_its_own_writable_tls_block() {
    let mut tls = Asm::default();
    tls.mov_reg(9, 0)
        .mrs_tpidrro(10)
        .str(10, 9, 0)
        .str(9, 10, 0)
        .svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov_reg(20, 0)
        .spawn(function(1), DATA + 8, PRIORITY, 1)
        .spawn(function(1), DATA + 16, PRIORITY, 2)
        .mov_reg(0, 20);
    main.0.extend_from_slice(&tls.0);

    let (mem, result) = run(&[&main, &tls]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    for thread in 0..3 {
        let block = TLS_BASE.get() + thread * TLS_SIZE;
        assert_eq!(
            data(&mem, 8 * thread),
            block,
            "TPIDRRO_EL0 of thread {thread}"
        );
        let written = mem.read::<u64>(GuestVa::new(block)).expect("test setup");
        assert_eq!(written, DATA + 8 * thread);
    }
}

#[test]
fn sleep_thread_parks_the_thread_for_at_least_the_time_asked() {
    const SLEEP: Duration = Duration::from_millis(20);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .mov(0, 0)
        .svc(SLEEP_THREAD)
        .mov(0, u64::try_from(SLEEP.as_nanos()).expect("test setup"))
        .svc(SLEEP_THREAD)
        .mov(10, 1)
        .str(10, 9, 0)
        .svc(EXIT_THREAD);

    let start = Instant::now();
    let (mem, result) = run(&[&main]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert!(start.elapsed() >= SLEEP);
    assert_eq!(data(&mem, 0), 1);
}

/// `ExitProcess` stops a thread spinning on another core; `run_cores` returns only after it
/// joined every core thread.
#[test]
fn exit_process_stops_every_core_and_joins_them() {
    let mut spinner = Asm::default();
    spinner.spin();
    let mut main = Asm::default();
    main.spawn(function(1), 0, PRIORITY, 3)
        .mov(0, 1_000_000)
        .svc(SLEEP_THREAD)
        .svc(EXIT_PROCESS);

    let (_, result) = run(&[&main, &spinner]);

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
}

#[test]
fn a_thread_on_a_core_that_does_not_exist_is_unimplemented() {
    let mut main = Asm::default();
    main.spawn(function(0), 0, PRIORITY, 6);

    let (_, result) = run(&[&main]);

    assert!(matches!(
        result,
        Err(RunError::Kernel(KernelError::Unimplemented {
            number: 0x08,
            name: "CreateThread"
        }))
    ));
}

#[test]
fn run_cores_without_a_started_thread_is_an_error() {
    let mem = space_with(&[]);
    assert!(matches!(
        run_cores(&mut kernel(), &mem),
        Err(RunError::NoThreads)
    ));
}

#[test]
fn spawn_thread_rejects_a_priority_or_core_out_of_range() {
    let mem = space_with(&[]);
    let mut kernel = kernel();
    let low = ThreadParams {
        priority: 0x40,
        ..main_thread(0)
    };
    let far = ThreadParams {
        core: 6,
        ..main_thread(0)
    };
    assert!(matches!(
        kernel.spawn_thread(&mem, low),
        Err(KernelError::BadThread { priority: 0x40, .. })
    ));
    assert!(matches!(
        kernel.spawn_thread(&mem, far),
        Err(KernelError::BadThread { core: 6, .. })
    ));
}

/// The single-thread B1 path has no current thread, so thread SVCs are unimplemented there.
#[test]
fn thread_svcs_are_unimplemented_without_a_current_thread() {
    let mem = space_with(&[]);
    let mut regs = Regs::default();
    assert!(matches!(
        kernel().dispatch(&mem, 0x0A, &mut regs),
        Err(KernelError::Unimplemented {
            name: "ExitThread",
            ..
        })
    ));
}

/// One thread sleeps 1 ns over and over on core 1 while another keeps moving it between cores
/// 3 and 1: its registers survive every move, so it finishes its loop and reports.
#[test]
fn moving_a_sleeping_thread_from_another_core_keeps_its_state() {
    const ROUNDS: u64 = 2_000;
    let mut sleeper = Asm::default();
    sleeper.mov_reg(9, 0).mov(11, ROUNDS);
    let top = sleeper.here();
    sleeper
        .mov(0, 1)
        .svc(SLEEP_THREAD)
        .sub_imm(11, 11, 1)
        .cbnz_back(11, top)
        .mov(10, 1)
        .str(10, 9, 0)
        .svc(EXIT_THREAD);
    let mut mover = Asm::default();
    mover.mov_reg(9, 0).mov(11, ROUNDS);
    let top = mover.here();
    for (core, mask) in [(3, 0b1000), (1, 0b10)] {
        mover
            .mov_reg(0, 9)
            .mov(1, core)
            .mov(2, mask)
            .svc(SET_THREAD_CORE_MASK);
    }
    mover.sub_imm(11, 11, 1).cbnz_back(11, top).svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov(1, function(1))
        .mov(2, DATA)
        .mov(3, STACK)
        .mov(4, u64::from(PRIORITY))
        .mov(5, 1)
        .svc(CREATE_THREAD)
        .mov_reg(21, 1)
        .mov_reg(0, 1)
        .svc(START_THREAD)
        // The mover's argument is the sleeper's handle, so `spawn` cannot create it.
        .mov(1, function(2))
        .mov_reg(2, 21)
        .mov(3, STACK)
        .mov(4, u64::from(PRIORITY))
        .mov(5, 2)
        .svc(CREATE_THREAD)
        .mov_reg(0, 1)
        .svc(START_THREAD)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &sleeper, &mover]);

    assert_eq!(
        result.expect("the sleeper keeps its state"),
        Finish::AllThreadsExited
    );
    assert_eq!(data(&mem, 0), 1);
}

/// How long a test waits for the guest before it gives up.
const PATIENCE: Duration = Duration::from_secs(5);

/// Polls `done` until it holds or [`PATIENCE`] runs out; stops the process on a timeout so
/// `run_cores_with` can return and the test can fail instead of hanging.
fn wait_for(ticker: &Ticker<'_, '_>, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while !done() {
        if start.elapsed() > PATIENCE {
            ticker.stop();
            return false;
        }
        thread::yield_now();
    }
    true
}

/// Runs `functions` as [`run`] does, with quanta ending only when `driver` ticks.
fn run_ticked(
    functions: &[&Asm],
    driver: impl FnOnce(&AddressSpace, &Ticker<'_, '_>),
) -> (AddressSpace, Result<Finish, RunError>) {
    let mem = space_with(functions);
    let mut kernel = kernel();
    kernel
        .spawn_thread(&mem, main_thread(DATA))
        .expect("test setup");
    let result = run_cores_with(&mut kernel, &mem, |ticker| driver(&mem, ticker));
    (mem, result)
}

/// ADR-0032 validation: two priority-44 threads on one core (44 is not sliced). The first
/// spins through every quantum and the second never runs until the first yields.
#[test]
fn an_unsliced_spinner_keeps_its_core_until_it_yields() {
    let mut spinner = Asm::default();
    spinner
        .mov_reg(9, 0)
        .mov(10, 0xA)
        .str(10, 9, 0)
        .wait_for(9, 16, 1);
    spinner.mov(0, 0).svc(SLEEP_THREAD).spin();
    let mut other = Asm::default();
    other.mov_reg(9, 0).mov(10, 0xB).str(10, 9, 8).spin();
    let mut main = Asm::default();
    main.spawn(function(1), DATA, PRIORITY, 1)
        .spawn(function(2), DATA, PRIORITY, 1)
        .svc(EXIT_THREAD);
    let mut seen = Vec::new();

    let (_, result) = run_ticked(&[&main, &spinner, &other], |mem, ticker| {
        seen.push(wait_for(ticker, || data(mem, 0) == 0xA));
        for _ in 0..5 {
            ticker.tick();
            thread::sleep(Duration::from_millis(2));
        }
        seen.push(data(mem, 8) == 0);
        mem.write(GuestVa::new(DATA + 16), 1u64)
            .expect("test setup");
        seen.push(wait_for(ticker, || data(mem, 8) == 0xB));
        ticker.stop();
    });

    assert_eq!(result.expect("test setup"), Finish::Stopped);
    assert_eq!(
        seen,
        [true, true, true],
        "started, kept its core, then yielded"
    );
}

/// ADR-0032 validation: two priority-59 threads (59 is sliced on core 1) take turns, one
/// per tick. Each keeps writing its id into the same word.
#[test]
fn a_sliced_pair_alternates_on_every_tick() {
    let mut a = Asm::default();
    a.mov_reg(9, 0).mov(10, 0xA);
    let top = a.here();
    a.str(10, 9, 0).b_back(top);
    let mut b = Asm::default();
    b.mov_reg(9, 0).mov(10, 0xB);
    let top = b.here();
    b.str(10, 9, 0).b_back(top);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .spawn(function(1), DATA, 59, 1)
        .spawn(function(2), DATA, 59, 1)
        .mov(10, 1)
        .str(10, 9, 8)
        .svc(EXIT_THREAD);
    let mut turns = Vec::new();

    let (_, result) = run_ticked(&[&main, &a, &b], |mem, ticker| {
        // Both threads exist before the first tick, or it would rotate a lone thread.
        let mut ok = wait_for(ticker, || data(mem, 8) == 1 && data(mem, 0) == 0xA);
        turns.push(data(mem, 0));
        for _ in 0..4 {
            let before = data(mem, 0);
            ticker.tick();
            ok &= wait_for(ticker, || data(mem, 0) != before);
            turns.push(data(mem, 0));
        }
        assert!(ok || ticker.is_finished());
        ticker.stop();
    });

    assert_eq!(result.expect("test setup"), Finish::Stopped);
    assert_eq!(turns, [0xA, 0xB, 0xA, 0xB, 0xA]);
}

/// ADR-0032 validation: a priority-0 thread started from core 0 onto core 2 interrupts the
/// priority-44 spinner there. No tick ever comes, so only the interrupt flag can free the core.
#[test]
fn a_cross_core_start_interrupts_a_spinner_without_a_tick() {
    let mut spinner = Asm::default();
    spinner.mov(10, 1).str(10, 0, 8).spin();
    let mut urgent = Asm::default();
    urgent.mov(10, 1).str(10, 0, 16).svc(EXIT_PROCESS);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .spawn(function(1), DATA, PRIORITY, 2)
        .wait_for(9, 8, 1)
        .spawn(function(2), DATA, 0, 2)
        .spin();

    let (mem, result) = run_ticked(&[&main, &spinner, &urgent], |_, ticker| {
        wait_for(ticker, || ticker.is_finished());
    });

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
    assert_eq!(data(&mem, 16), 1, "the urgent thread ran");
}

/// ADR-0032 validation: moving a running thread to another core takes it off its old core
/// before the new one runs it; its registers survive and it reports the new core.
#[test]
fn moving_a_running_thread_hands_it_over_between_cores() {
    let mut mover = Asm::default();
    mover.mov_reg(9, 0).mov(20, 0);
    let top = mover.here();
    mover
        .svc(GET_CURRENT_PROCESSOR_NUMBER)
        .str(0, 9, 0)
        .add_imm(20, 20, 1)
        .str(20, 9, 8)
        .b_back(top);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .mov(1, function(1))
        .mov(2, DATA)
        .mov(3, STACK)
        .mov(4, u64::from(PRIORITY))
        .mov(5, 2)
        .svc(CREATE_THREAD)
        .mov_reg(21, 1)
        .mov_reg(0, 1)
        .svc(START_THREAD)
        .wait_for(9, 0, 2)
        .mov_reg(0, 21)
        .mov(1, 3)
        .mov(2, 0b1000)
        .svc(SET_THREAD_CORE_MASK)
        .wait_for(9, 0, 3)
        .ldr(22, 9, 8)
        .str(22, 9, 16)
        .svc(EXIT_PROCESS);

    let (mem, result) = run_ticked(&[&main, &mover], |_, ticker| {
        wait_for(ticker, || ticker.is_finished());
    });

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
    assert!(
        data(&mem, 16) > 0,
        "its loop counter carried over to core 3"
    );
}

/// A driver that returns while a guest thread still spins stops the process, so
/// `run_cores_with` returns instead of waiting forever.
#[test]
fn run_cores_with_stops_the_process_when_its_driver_returns() {
    let mut main = Asm::default();
    main.spin();

    let (_, result) = run_ticked(&[&main], |_, _| {});

    assert_eq!(result.expect("test setup"), Finish::Stopped);
}

/// With the kernel timer keeping time, a priority-10 sleeper wakes on time on a core that a
/// priority-44 spinner keeps busy. The quantum is far longer than the sleep, so only the
/// timer's wakeup fire can end it.
#[test]
fn the_timer_wakes_a_sleeper_on_a_busy_core() {
    const SLEEP: Duration = Duration::from_millis(30);
    let mut spinner = Asm::default();
    spinner.mov(10, 1).str(10, 0, 8).spin();
    let mut sleeper = Asm::default();
    sleeper
        .mov_reg(9, 0)
        .mov(0, u64::try_from(SLEEP.as_nanos()).expect("test setup"))
        .svc(SLEEP_THREAD)
        .mov(10, 1)
        .str(10, 9, 16)
        .svc(EXIT_PROCESS);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .spawn(function(1), DATA, PRIORITY, 1)
        .wait_for(9, 8, 1)
        .spawn(function(2), DATA, 10, 1)
        .svc(EXIT_THREAD);

    let mem = space_with(&[&main, &spinner, &sleeper]);
    let mut kernel = kernel();
    kernel.set_quantum(PATIENCE * 4);
    kernel
        .spawn_thread(&mem, main_thread(DATA))
        .expect("test setup");

    let start = Instant::now();
    let result = run_cores(&mut kernel, &mem);

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
    assert_eq!(data(&mem, 16), 1);
    let elapsed = start.elapsed();
    assert!(
        elapsed >= SLEEP && elapsed < PATIENCE,
        "woke after {elapsed:?}"
    );
}

/// With the kernel timer keeping time, two priority-59 spinners on core 1 both run: the
/// second only gets the core when a quantum rotates the first out. The main thread exits once
/// both have marked their word.
#[test]
fn the_timer_rotates_a_sliced_pair() {
    let mut marker = Asm::default();
    marker.mov(10, 1).str(10, 0, 0).spin();
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .spawn(function(1), DATA, 59, 1)
        .spawn(function(1), DATA + 8, 59, 1)
        .wait_for(9, 0, 1)
        .wait_for(9, 8, 1)
        .svc(EXIT_PROCESS);

    let (mem, result) = run(&[&main, &marker]);

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
    assert_eq!((data(&mem, 0), data(&mem, 8)), (1, 1), "both took turns");
}

/// ADR-0032 ledger: core threads record how long they hold the kernel lock, and a core that
/// switches after a cross-core start records the latency from the request to the switch.
#[test]
fn core_threads_record_lock_hold_and_preemption_latency() {
    let mut spinner = Asm::default();
    spinner.mov(10, 1).str(10, 0, 8).spin();
    let mut urgent = Asm::default();
    urgent.svc(EXIT_PROCESS);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .spawn(function(1), DATA, PRIORITY, 2)
        .wait_for(9, 8, 1)
        .spawn(function(2), DATA, 0, 2)
        .spin();
    let mem = space_with(&[&main, &spinner, &urgent]);
    let mut trace = Trace::new(Trace::MAX_CAPACITY);
    let mut kernel = Kernel::new(&mut trace).expect("test setup");
    kernel
        .spawn_thread(&mem, main_thread(DATA))
        .expect("test setup");

    let result = run_cores(&mut kernel, &mem);

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
    let drained = trace.drain();
    let count = |kind| {
        drained
            .records
            .iter()
            .filter(|record| record.role == ThreadRole::GuestCore && record.event.kind == kind)
            .count()
    };
    assert!(count(EventKind::KernelLockHeld) > 0, "lock holds recorded");
    assert!(
        count(EventKind::PreemptLatency) > 0,
        "the spinner's core recorded its switch"
    );
}

/// Waiters of priorities 40, 20 and 30 block on one event; once it is signaled they all wake
/// and run in priority order, each seeing the signal and a successful wait on index 0.
#[test]
fn waiters_on_an_event_wake_in_priority_order() {
    let mut waiter = Asm::default();
    waiter
        .mov_reg(20, 0)
        .mov(9, DATA)
        .wait(9, 1, -1)
        // Accumulate (signal flag - 1) | W0 | W1 at 0x48: zero when every waiter saw the
        // signal and a successful wait on index 0. Same-core waiters run one at a time.
        .ldr(21, 9, 0x40)
        .sub_imm(21, 21, 1)
        .orr(21, 21, 0)
        .orr(21, 21, 1)
        .ldr(22, 9, 0x48)
        .orr(22, 22, 21)
        .str(22, 9, 0x48)
        .append(9, 20)
        .svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .mov(0, u64::from(CURRENT_THREAD))
        .mov(1, 10)
        .svc(SET_THREAD_PRIORITY)
        .svc(CREATE_EVENT)
        .mov_reg(22, 1)
        .str_w(2, 9, HANDLES);
    for priority in [40, 20, 30] {
        main.spawn(function(1), u64::from(priority), priority, 0);
    }
    main.mov(0, 10_000_000)
        .svc(SLEEP_THREAD)
        .mov(10, 1)
        .str(10, 9, 0x40)
        .mov_reg(0, 22)
        .svc(SIGNAL_EVENT)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &waiter]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    let list: Vec<u64> = (0..4).map(|n| data(&mem, 8 * n)).collect();
    assert_eq!(list, [3, 20, 30, 40]);
    assert_eq!(
        data(&mem, 0x48),
        0,
        "each saw the signal, W0 = 0 and W1 = 0"
    );
}

/// A wait with a timeout ends with `TIMED_OUT` once it passes; a zero timeout only polls.
#[test]
fn a_wait_times_out_and_a_zero_timeout_polls() {
    const TIMEOUT: Duration = Duration::from_millis(20);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .svc(CREATE_EVENT)
        .str_w(2, 9, HANDLES)
        .wait(9, 1, 0)
        .str(0, 9, 0)
        .wait(9, 1, i64::try_from(TIMEOUT.as_nanos()).expect("test setup"))
        .str(0, 9, 8)
        .svc(EXIT_THREAD);

    let start = Instant::now();
    let (mem, result) = run(&[&main]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!((data(&mem, 0), data(&mem, 8)), (TIMED_OUT, TIMED_OUT));
    assert!(start.elapsed() >= TIMEOUT);
}

/// `CancelSynchronization` ends a worker's endless wait with `CANCELLED`, whether it finds the
/// worker waiting or the cancellation waits for the worker's next wait. On the caller itself
/// it cancels the next wait only.
#[test]
fn cancel_synchronization_ends_a_wait_now_or_the_next_one() {
    let mut worker = Asm::default();
    worker
        .mov(9, DATA)
        .wait(9, 1, -1)
        .str(0, 9, 0)
        .svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .svc(CREATE_EVENT)
        .str_w(2, 9, HANDLES)
        .spawn(function(1), 0, PRIORITY, 1)
        .mov_reg(0, 1)
        .svc(CANCEL_SYNCHRONIZATION)
        .mov(0, u64::from(CURRENT_THREAD))
        .svc(CANCEL_SYNCHRONIZATION)
        .wait(9, 1, -1)
        .str(0, 9, 8)
        .wait(9, 1, 0)
        .str(0, 9, 16)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &worker]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!(data(&mem, 0), CANCELLED, "the worker's wait");
    assert_eq!(data(&mem, 8), CANCELLED, "the caller's next wait");
    assert_eq!(data(&mem, 16), TIMED_OUT, "the pending cancel was used up");
}

/// A thread is signaled once it exits; the waiter learns its position in the handle list.
#[test]
fn waiting_on_a_thread_ends_when_it_exits() {
    let mut worker = Asm::default();
    worker.mov(0, 5_000_000).svc(SLEEP_THREAD).svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .svc(CREATE_EVENT)
        .str_w(2, 9, HANDLES)
        .spawn(function(1), 0, PRIORITY, 1)
        .str_w(1, 9, HANDLES + 4)
        .wait(9, 2, -1)
        .str(0, 9, 0)
        .str(1, 9, 8)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &worker]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!((data(&mem, 0), data(&mem, 8)), (0, 1));
}

/// Bad handle lists fail before any wait: too many handles, an unreadable array, or a handle
/// that is not waitable.
#[test]
fn wait_synchronization_rejects_bad_handle_lists() {
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .svc(CREATE_EVENT)
        .str_w(1, 9, HANDLES)
        .wait(9, 0x41, -1)
        .str(0, 9, 0)
        .mov(1, 0)
        .mov(2, 1)
        .mov(3, u64::MAX)
        .svc(WAIT_SYNCHRONIZATION)
        .str(0, 9, 8)
        .wait(9, 1, -1)
        .str(0, 9, 16)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    let codes = (data(&mem, 0), data(&mem, 8), data(&mem, 16));
    assert_eq!(
        codes,
        (0xEE01, 0xE601, 0xE401),
        "too many, bad address, writable end"
    );
}

/// Runs `functions` like [`run`], after `setup` saw the space and the main thread's handle.
fn run_with_main(
    functions: &[&Asm],
    setup: impl FnOnce(&AddressSpace, u64),
) -> (AddressSpace, Result<Finish, RunError>) {
    let mem = space_with(functions);
    let mut kernel = kernel();
    let main = kernel
        .spawn_thread(&mem, main_thread(DATA))
        .expect("test setup");
    setup(&mem, u64::from(main));
    let result = run_cores(&mut kernel, &mem);
    (mem, result)
}

fn set_word(mem: &AddressSpace, offset: u32, value: u64) {
    let value = u32::try_from(value).expect("test setup");
    mem.write(GuestVa::new(DATA + u64::from(offset)), value)
        .expect("test setup");
}

fn word(mem: &AddressSpace, offset: u32) -> u64 {
    u64::from(
        mem.read::<u32>(GuestVa::new(DATA + u64::from(offset)))
            .expect("test setup"),
    )
}

/// The main thread holds a contended mutex; waiters of priorities 40, 20 and 30 block in
/// `ArbitrateLock`. Each unlock hands the mutex to the best waiter, which returns owning it,
/// until the last unlock frees it.
#[test]
fn arbitrate_unlock_hands_the_mutex_to_waiters_in_priority_order() {
    let mut waiter = Asm::default();
    waiter
        .mov_reg(20, 0)
        .mov(9, DATA)
        .ldr(21, 9, 0x60)
        .lock(9, 21, 20)
        .ldr(22, 9, 0x48)
        .orr(22, 22, 0)
        .str(22, 9, 0x48)
        .ldr_w(23, 9, MUTEX)
        .str(23, 9, 0x68)
        .append(9, 20)
        .unlock(9)
        .svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .mov(0, u64::from(CURRENT_THREAD))
        .mov(1, 10)
        .svc(SET_THREAD_PRIORITY);
    for priority in [40, 20, 30] {
        main.spawn(function(1), u64::from(priority), priority, 0);
    }
    main.mov(0, 10_000_000)
        .svc(SLEEP_THREAD)
        .unlock(9)
        .svc(EXIT_THREAD);

    let (mem, result) = run_with_main(&[&main, &waiter], |mem, main| {
        set_word(mem, MUTEX, main | WAITERS);
        mem.write(GuestVa::new(DATA + 0x60), main)
            .expect("test setup");
    });

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    let list: Vec<u64> = (0..4).map(|n| data(&mem, 8 * n)).collect();
    assert_eq!(list, [3, 20, 30, 40]);
    assert_eq!(data(&mem, 0x48), 0, "every lock returned success");
    assert_eq!(data(&mem, 0x68), 40, "the last owner held it uncontended");
    assert_eq!(word(&mem, MUTEX), 0, "the last unlock freed it");
}

/// `ArbitrateLock` returns at once when the word no longer names the owner with waiters.
#[test]
fn arbitrate_lock_returns_at_once_when_the_mutex_changed() {
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .ldr(21, 9, 0x60)
        .mov(22, 7)
        .lock(9, 21, 22)
        .str(0, 9, 0)
        .svc(EXIT_THREAD);

    let (mem, result) = run_with_main(&[&main], |mem, main| {
        set_word(mem, MUTEX, main);
        mem.write(GuestVa::new(DATA + 0x60), main)
            .expect("test setup");
    });

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!(data(&mem, 0), 0);
    assert_eq!(word(&mem, MUTEX), data(&mem, 0x60), "untouched");
}

/// Two waiters (priorities 30 and 20) take the mutex and wait on a key, which releases it.
/// Signaling one waiter wakes priority 20 with the mutex free; signaling all wakes priority
/// 30 into a mutex waiter, and the key reads 0 again. The unlocks then hand the mutex on.
#[test]
fn a_signaled_condition_variable_waiter_reacquires_its_mutex() {
    let mut waiter = Asm::default();
    waiter
        .mov_reg(20, 0)
        .mov(9, DATA)
        .str_w(20, 9, MUTEX)
        .wait_key(9, 20, -1)
        .ldr(22, 9, 0x48)
        .orr(22, 22, 0)
        .str(22, 9, 0x48)
        .append(9, 20)
        .unlock(9)
        .svc(EXIT_THREAD);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .mov(0, u64::from(CURRENT_THREAD))
        .mov(1, 10)
        .svc(SET_THREAD_PRIORITY);
    for priority in [30, 20] {
        main.spawn(function(1), u64::from(priority), priority, 0);
    }
    main.mov(0, 10_000_000)
        .svc(SLEEP_THREAD)
        .ldr_w(10, 9, KEY)
        .str(10, 9, 0x70)
        .ldr_w(10, 9, MUTEX)
        .str(10, 9, 0x78)
        .signal_key(9, 1)
        .ldr_w(10, 9, KEY)
        .str(10, 9, 0x80 + 0x20)
        .signal_key(9, -1)
        .ldr_w(10, 9, KEY)
        .str(10, 9, 0x80 + 0x28)
        .ldr_w(10, 9, MUTEX)
        .str(10, 9, 0x80 + 0x30)
        .svc(EXIT_THREAD);

    let (mem, result) = run(&[&main, &waiter]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!(
        (data(&mem, 0x70), data(&mem, 0x78)),
        (1, 0),
        "both waiting, mutex free"
    );
    assert_eq!(data(&mem, 0xA0), 1, "one waiter left on the key");
    assert_eq!(data(&mem, 0xA8), 0, "no waiter left on the key");
    assert_eq!(
        data(&mem, 0xB0),
        0x14 | WAITERS,
        "20 owns it, 30 waits for it"
    );
    let list: Vec<u64> = (0..3).map(|n| data(&mem, 8 * n)).collect();
    assert_eq!(list, [2, 20, 30]);
    assert_eq!(data(&mem, 0x48), 0, "every wait returned success");
    assert_eq!(word(&mem, MUTEX), 0);
}

/// A condition-variable wait that times out returns `TIMED_OUT` without the mutex (GUESS).
#[test]
fn a_timed_out_condition_variable_wait_returns_without_the_mutex() {
    const TIMEOUT: Duration = Duration::from_millis(20);
    let mut main = Asm::default();
    main.mov_reg(9, 0)
        .mov(20, 5)
        .str_w(20, 9, MUTEX)
        .wait_key(
            9,
            20,
            i64::try_from(TIMEOUT.as_nanos()).expect("test setup"),
        )
        .str(0, 9, 0)
        .ldr_w(10, 9, MUTEX)
        .str(10, 9, 8)
        .svc(EXIT_THREAD);

    let start = Instant::now();
    let (mem, result) = run(&[&main]);

    assert_eq!(result.expect("test setup"), Finish::AllThreadsExited);
    assert_eq!((data(&mem, 0), data(&mem, 8)), (TIMED_OUT, 0));
    assert!(start.elapsed() >= TIMEOUT);
}

/// Defers every request to the service-I/O thread, where the work waits until `release` is
/// set, then replies with `0x5A` in the first word.
struct Deferring {
    name: NameId,
    release: Arc<AtomicBool>,
}

impl SessionObject for Deferring {
    fn name(&self) -> &'static str {
        "deferring"
    }

    fn trace_name(&self) -> NameId {
        self.name
    }

    fn handle_request(
        &mut self,
        sessions: &mut dyn Sessions,
        _: &mut IpcMessage,
    ) -> Result<(), UnknownCommand> {
        let release = Arc::clone(&self.release);
        let work = Box::new(move || {
            while !release.load(Ordering::Acquire) {
                thread::yield_now();
            }
            let mut reply = [0; IPC_MESSAGE_SIZE];
            reply[0] = 0x5A;
            DeferredReply::Keep(reply)
        });
        if sessions.defer("deferring", work) {
            Ok(())
        } else {
            Err(UnknownCommand { command: 0 })
        }
    }
}

const CONNECT_TO_NAMED_PORT: u16 = 0x1F;
const SEND_SYNC_REQUEST: u16 = 0x21;
/// Where the guest finds the port name.
const PORT_NAME_AT: u64 = 0x200;

/// A kernel with the `deferring` port and a service-I/O thread, a space holding `functions`
/// and the port name, and the main thread.
fn deferring_setup(functions: &[&Asm], release: &Arc<AtomicBool>) -> (Kernel, AddressSpace) {
    let mem = space_with(functions);
    mem.write_bytes(GuestVa::new(DATA + PORT_NAME_AT), b"deferring\0")
        .expect("test setup");
    let mut trace = Trace::new(64);
    let mut kernel = Kernel::new(&mut trace).expect("test setup");
    let name = trace.name("deferring").expect("test setup");
    let release = Arc::clone(release);
    kernel.register_named_port(
        "deferring",
        Box::new(move || {
            Box::new(Deferring {
                name,
                release: Arc::clone(&release),
            })
        }),
    );
    kernel.start_service_io(4).expect("test setup");
    kernel
        .spawn_thread(&mem, main_thread(DATA))
        .expect("test setup");
    (kernel, mem)
}

/// The main thread: connects to `deferring`, sends one request and stores `W0` at `+0x10`
/// and the reply's first word at `+0x18`, then exits the process. `+0x10` holds `0xFFFF`
/// until the reply arrives.
fn deferring_client(worker: Option<u64>) -> Asm {
    let mut main = Asm::default();
    main.mov_reg(9, 0).mov(13, 0xFFFF).str(13, 9, 0x10);
    if let Some(entry) = worker {
        main.spawn(entry, DATA, PRIORITY, 1);
    }
    main.mov(1, DATA + PORT_NAME_AT)
        .svc(CONNECT_TO_NAMED_PORT)
        .mov_reg(0, 1)
        .svc(SEND_SYNC_REQUEST)
        .str(0, 9, 0x10)
        .mrs_tpidrro(11)
        .ldr_w(12, 11, 0)
        .str(12, 9, 0x18)
        .svc(EXIT_PROCESS);
    main
}

/// T-063: while a guest thread waits for a reply built on the service-I/O thread, another
/// guest thread keeps running; the reply then reaches the waiter through the kernel.
#[test]
fn a_deferred_reply_wakes_its_thread_while_others_keep_running() {
    let mut counter = Asm::default();
    counter.mov_reg(9, 0).mov(10, 0);
    let top = counter.here();
    counter.add_imm(10, 10, 1).str(10, 9, 8).b_back(top);
    let release = Arc::new(AtomicBool::new(false));
    let (mut kernel, mem) =
        deferring_setup(&[&deferring_client(Some(function(1))), &counter], &release);

    let result = run_cores_with(&mut kernel, &mem, |ticker| {
        let blocked = wait_for(ticker, || data(&mem, 8) > 0);
        let before = data(&mem, 8);
        let ran = wait_for(ticker, || data(&mem, 8) > before + 10_000);
        assert!(
            blocked && ran,
            "the counter thread ran while the request waited"
        );
        assert_eq!(data(&mem, 0x10), 0xFFFF, "no reply yet");
        release.store(true, Ordering::Release);
        assert!(
            wait_for(ticker, || {
                ticker.tick();
                ticker.is_finished()
            }),
            "the reply reached the main thread"
        );
    });

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
    assert_eq!((data(&mem, 0x10), data(&mem, 0x18)), (0, 0x5A));
    kernel
        .shutdown_service_io()
        .expect("started")
        .expect("joined");
}

/// Under [`run_cores`] the kernel timer delivers the reply as soon as the work is done.
#[test]
fn the_kernel_timer_delivers_a_deferred_reply() {
    let release = Arc::new(AtomicBool::new(true));
    let (mut kernel, mem) = deferring_setup(&[&deferring_client(None)], &release);

    let result = run_cores(&mut kernel, &mem);

    assert_eq!(result.expect("test setup"), Finish::ExitProcess);
    assert_eq!((data(&mem, 0x10), data(&mem, 0x18)), (0, 0x5A));
    assert_eq!(
        kernel
            .shutdown_service_io()
            .expect("started")
            .expect("joined")
            .completed,
        1
    );
}
