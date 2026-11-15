// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `lince` frontend: runs a guest program and shows its output on the host.
//!
//! # Role
//! B2 scope (doc 07): [`run_nro`] loads a homebrew NRO into a fresh address space, starts its
//! main thread and runs it on the guest core threads ([`run_cores`]) until it exits, with
//! `sm:` and the B2 service stubs registered. The guest's `OutputDebugString` text goes to a
//! host writer. With no `lince-core` yet, this crate wires
//! the subsystems together itself. In coverage mode ([`Options::coverage`], T-055) unknown
//! SVCs and IPC commands do not stop the run; [`Report`] lists them at exit.
//!
//! # Threads
//! The guest runs on the kernel's guest core threads; the calling thread waits for them. The
//! debug output is written on the calling thread after the run, never on a guest core.
//!
//! # References
//! ADR-0001 · ADR-0005 · ADR-0017 · ADR-0024 · docs/07-development-roadmap.md (B1, B2)

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use lince_common::GuestVa;
use lince_hle::fsp::SdCard;
use lince_hle::{sm, stubs};
use lince_kernel::{Kernel, KernelError, RunError, ThreadParams, run_cores};
use lince_loader::{LoaderError, load_nro};
use lince_mem::{AddressSpace, MemError, Permission, SpaceConfig};
use lince_sched::SchedError;
use lince_trace::{Trace, TraceError, Unimplemented};
use thiserror::Error;

/// Requests the service-I/O queue holds before deferring services wait (T-063).
const SERVICE_IO_QUEUE: usize = 16;
/// Where the NRO is loaded; away from the kernel's heap base.
const LOAD_BASE: GuestVa = GuestVa::new(0x800_0000);
/// The main thread's stack, below the load base.
const STACK_BASE: GuestVa = GuestVa::new(0x700_0000);
const STACK_SIZE: u64 = 0x1_0000;
/// The main thread's priority and core.
// GUESS: 44 is the usual main-thread priority of homebrew; nothing in B2 depends on it.
const MAIN_PRIORITY: u32 = 44;
const MAIN_CORE: usize = 0;
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
    /// The kernel could not be set up or start the main thread.
    #[error("{0}")]
    Kernel(#[from] KernelError),
    /// The services could not be registered with the trace.
    #[error("cannot register the services: {0}")]
    Trace(#[from] TraceError),
    /// The run stopped before the program exited: a fault, `Break`, an SVC the kernel
    /// rejected or a host error.
    #[error("{0}")]
    Run(#[from] RunError),
    /// The SD-card directory cannot be served.
    #[error("cannot use {} as the SD card: {source}", path.display())]
    SdCard {
        /// The directory given.
        path: PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// The service-I/O thread could not start, or panicked.
    #[error("service-I/O thread: {0}")]
    ServiceIo(#[from] SchedError),
    /// Writing the guest's output failed.
    #[error("cannot write the guest output: {0}")]
    Output(#[from] std::io::Error),
}

/// How [`run_nro`] runs a program.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Coverage mode: an unknown SVC or IPC command returns an error to the guest instead of
    /// stopping the run ([`Kernel::set_keep_going`]).
    pub coverage: bool,
    /// A host directory `fsp-srv` serves as the SD card (read-only, T-064); without it the
    /// SD card is a stub with no commands.
    pub sd: Option<PathBuf>,
}

/// What a run that reached its exit reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Every unknown SVC and IPC command the run reached, each once.
    pub unimplemented: Vec<Unimplemented>,
    /// Once-only trace records (unknown calls, stubs) lost because a buffer was full; when
    /// non-zero the list may be incomplete. Lost plain records (scheduler ledger events) do not
    /// count: they never hold an unknown call.
    pub dropped: u64,
}

/// Writes the coverage list in `report`: a count line, then one line per call.
pub fn write_coverage(out: &mut impl Write, report: &Report) -> std::io::Result<()> {
    writeln!(
        out,
        "coverage: {} unimplemented calls",
        report.unimplemented.len()
    )?;
    for call in &report.unimplemented {
        writeln!(out, "  {call}")?;
    }
    if report.dropped > 0 {
        writeln!(
            out,
            "coverage: {} once-only trace records dropped; the list may be incomplete",
            report.dropped
        )?;
    }
    Ok(())
}

/// Loads `file` as an NRO, runs it until it calls `ExitProcess` and writes everything it
/// prints with `OutputDebugString` to `out`.
pub fn run_nro(file: &[u8], out: &mut impl Write, options: &Options) -> Result<Report, AppError> {
    let mut mem = AddressSpace::new(SpaceConfig::for_host())?;
    let nro = load_nro(&mut mem, LOAD_BASE, file)?;
    mem.map(STACK_BASE, STACK_SIZE, Permission::READ | Permission::WRITE)?;
    // Shared with `fsp-srv`, which fills read buffers from the service-I/O thread.
    let mem = Arc::new(mem);
    let mut trace = Trace::new(TRACE_CAPACITY);
    let mut kernel = Kernel::new(&mut trace)?;
    kernel.set_keep_going(options.coverage);
    let sd = match &options.sd {
        Some(path) => {
            Some(
                SdCard::new(path, Arc::clone(&mem)).map_err(|source| AppError::SdCard {
                    path: path.clone(),
                    source,
                })?,
            )
        }
        None => None,
    };
    if sd.is_some() {
        kernel.start_service_io(SERVICE_IO_QUEUE)?;
    }
    let services = stubs::b2_services(&mut trace, sd)?;
    sm::install(&mut kernel, &mut trace, services)?;
    let main = ThreadParams {
        entry: nro.entry,
        arg: 0,
        stack_top: STACK_BASE.get() + STACK_SIZE,
        priority: MAIN_PRIORITY,
        core: MAIN_CORE,
    };
    kernel.spawn_thread(&mem, main)?;
    let finished = run_cores(&mut kernel, &mem);
    let service_io = kernel.shutdown_service_io();
    // The output of a run that failed is written too: it shows how far the program got.
    out.write_all(&kernel.take_debug_output())?;
    finished?;
    service_io.transpose()?;
    out.flush()?;
    let drained = trace.drain();
    Ok(Report {
        unimplemented: trace.unimplemented(&drained),
        dropped: drained.dropped_once,
    })
}
