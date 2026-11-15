// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-kernel` (PAT-01).
//!
//! Failures a guest can observe (bad arguments, unmapped memory) are SVC result codes, not
//! these errors. These errors mean the dispatcher itself cannot complete the call.

use lince_cpu_a64::{CpuError, Exit};
use lince_trace::TraceError;
use thiserror::Error;

/// Errors from creating a [`Kernel`](crate::Kernel) or dispatching an SVC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum KernelError {
    /// The SVC number has no entry in the table.
    #[error("unknown SVC {number:#04x}")]
    UnknownSvc {
        /// The SVC number.
        number: u32,
    },
    /// The SVC is named, but this build has no behavior for it or for the arguments given.
    #[error("SVC {number:#04x} ({name}) is not implemented")]
    Unimplemented {
        /// The SVC number.
        number: u32,
        /// Its public or descriptive name.
        name: &'static str,
    },
    /// A session object received a command it has no behavior for (ADR-0024).
    #[error("{object} has no command {command}")]
    UnknownCommand {
        /// The object's public or descriptive name.
        object: &'static str,
        /// The command id, or the message type when the message has no command id.
        command: u32,
    },
    /// A host-created thread had a priority or core out of range, or its TLS block could not
    /// be mapped.
    #[error("cannot create a thread with priority {priority} on core {core}")]
    BadThread {
        /// The priority asked for.
        priority: u32,
        /// The core asked for.
        core: usize,
    },
    /// The names of the kernel's stubs could not be registered.
    #[error("cannot register kernel trace names: {0}")]
    Trace(#[from] TraceError),
}

/// Why [`run_cores`](crate::run_cores) stopped other than by a normal process exit.
#[derive(Debug, Error)]
pub enum RunError {
    /// No thread was started, so nothing could run.
    #[error("no thread was started")]
    NoThreads,
    /// The kernel timer thread could not be created.
    #[error("cannot create the kernel timer thread")]
    Timer(#[source] lince_sched::SchedError),
    /// A core thread could not be created.
    #[error("cannot create the thread of guest core {core}: {source}")]
    Spawn {
        /// The guest core.
        core: usize,
        /// The host error.
        #[source]
        source: std::io::Error,
    },
    /// A core thread panicked, a host bug; the panic itself reaches the caller of
    /// [`run_cores`](crate::run_cores) once every core is joined.
    #[error("the thread of guest core {core} panicked")]
    Panicked {
        /// The guest core.
        core: usize,
    },
    /// The kernel timer thread panicked; [`run_cores`](crate::run_cores) stops the cores.
    #[error("the kernel timer thread panicked")]
    TimerPanicked,
    /// The interpreter failed.
    #[error("interpreter error: {0}")]
    Cpu(#[from] CpuError),
    /// The kernel rejected an SVC.
    #[error("{0}")]
    Kernel(#[from] KernelError),
    /// A guest thread stopped on an exception the kernel does not handle.
    #[error("guest fault on core {core}: {exit:?}")]
    Fault {
        /// The guest core.
        core: usize,
        /// The interpreter exit.
        exit: Exit,
    },
    /// A guest thread aborted with `Break`.
    #[error("guest called Break with reason {reason:#x}")]
    Break {
        /// The reason in `X0`.
        reason: u64,
    },
}

/// `Result` with [`KernelError`] as the default error type.
pub type Result<T, E = KernelError> = std::result::Result<T, E>;
