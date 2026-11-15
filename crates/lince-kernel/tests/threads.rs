// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest threads on the six core threads (T-051): small guest programs create, start, move,
//! reprioritize and end threads through the thread SVCs, and record what they saw in memory.
//!
//! Encodings follow the A64 base instruction formats of the Arm Architecture Reference Manual.

use std::time::{Duration, Instant};

use lince_common::GuestVa;
use lince_kernel::{
    CURRENT_THREAD, Finish, Kernel, KernelError, Regs, RunError, TLS_BASE, TLS_SIZE, ThreadParams,
    run_cores,
};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};
use lince_trace::Trace;

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

/// Two workers on the main thread's core, started low priority first: the high-priority one
/// runs first once the main thread exits. Each appends its argument to a list at `DATA`.
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
    main.spawn(function(1), 0xA, 40, 0)
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
