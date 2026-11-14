// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `lince` frontend: runs a guest program and shows its output on the host.
//!
//! # Role
//! B1 scope (doc 07): [`run_nro`] loads a homebrew NRO into a fresh address space, runs it on
//! the Tier A interpreter and dispatches its SVCs to the kernel until it exits. The guest's
//! `OutputDebugString` text goes to a host writer. With no `lince-core` yet, this crate wires
//! the subsystems together itself.
//!
//! # Threads
//! The guest runs on the calling thread, which is its single guest core. Writing the debug
//! output is host I/O on that thread; B1 has no frame to protect.
//!
//! # References
//! ADR-0001 · ADR-0005 · ADR-0024 · docs/07-development-roadmap.md (B1)

use std::io::Write;

use lince_common::GuestVa;
use lince_cpu_a64::{CpuError, Exit, State, run};
use lince_kernel::{Flow, Kernel, KernelError, Regs};
use lince_loader::{LoaderError, load_nro};
use lince_mem::{AddressSpace, MemError, SpaceConfig};
use lince_trace::Trace;
use thiserror::Error;

/// Where the NRO is loaded; away from the kernel's heap base.
const LOAD_BASE: GuestVa = GuestVa::new(0x800_0000);
/// Instructions per interpreter call; the guest resumes after each limit exit.
const SLICE: u64 = 1 << 20;
/// Undrained trace events per recorder.
const TRACE_CAPACITY: usize = 64;

/// Why a guest program could not run to its exit.
#[derive(Debug, Error)]
pub enum AppError {
    /// The address space could not be created.
    #[error("cannot create the guest address space: {0}")]
    Mem(#[from] MemError),
    /// The file is not a loadable NRO.
    #[error("cannot load the program: {0}")]
    Loader(#[from] LoaderError),
    /// The interpreter failed.
    #[error("interpreter error: {0}")]
    Cpu(#[from] CpuError),
    /// The kernel rejected an SVC.
    #[error("{0}")]
    Kernel(#[from] KernelError),
    /// The guest stopped on an exception the kernel does not handle.
    #[error("guest fault: {exit:?}")]
    Fault {
        /// The interpreter exit.
        exit: Exit,
    },
    /// The guest aborted with `Break`.
    #[error("guest called Break with reason {reason:#x}")]
    Break {
        /// The reason in `X0`.
        reason: u64,
    },
    /// Writing the guest's output failed.
    #[error("cannot write the guest output: {0}")]
    Output(#[from] std::io::Error),
}

/// Loads `file` as an NRO, runs it until it calls `ExitProcess` and writes everything it
/// prints with `OutputDebugString` to `out`.
pub fn run_nro(file: &[u8], out: &mut impl Write) -> Result<(), AppError> {
    let mut mem = AddressSpace::new(SpaceConfig::for_host())?;
    let nro = load_nro(&mut mem, LOAD_BASE, file)?;
    let mut trace = Trace::new(TRACE_CAPACITY);
    let mut kernel = Kernel::new(&mut trace)?;
    let mut state = State {
        pc: nro.entry.get(),
        ..State::default()
    };
    loop {
        let result = run(&mut state, &mut mem, SLICE)?;
        let number = match result.exit {
            Exit::Svc { number } => number,
            Exit::Limit => continue,
            exit @ (Exit::Undefined { .. }
            | Exit::Unimplemented { .. }
            | Exit::FetchAbort { .. }
            | Exit::PcMisaligned { .. }
            | Exit::DataAbort { .. }) => return Err(AppError::Fault { exit }),
        };
        let number = u8::try_from(number).map_err(|_| KernelError::UnknownSvc {
            number: u32::from(number),
        })?;
        let mut regs = Regs::default();
        let args = regs.len();
        regs.copy_from_slice(&state.x[..args]);
        let flow = kernel.dispatch(&mut mem, number, &mut regs);
        state.x[..args].copy_from_slice(&regs);
        out.write_all(&kernel.take_debug_output())?;
        match flow? {
            Flow::Continue => {}
            Flow::Exit => break,
            Flow::Break { reason } => return Err(AppError::Break { reason }),
        }
    }
    out.flush()?;
    Ok(())
}
