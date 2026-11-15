// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Horizon-compatible kernel HLE: the SVC boundary and the kernel state behind it.
//!
//! # Role
//! B1 scope (ADR-0024): a table indexed by SVC number dispatches the memory SVCs, process exit,
//! `Break` (a named stub) and `OutputDebugString`. The Switch 2 SVC `0x80` is reserved by name.
//! An SVC with no handler returns a distinct error and is recorded once through `lince-trace`;
//! it never reports success. Numbers, arguments and results come from
//! docs/research/nro-and-b1-svcs.md (Switch 1 knowledge; Switch 2 values are UNKNOWN).
//!
//! B2 scope adds guest threads (T-051): [`run_cores`] runs them on six host core threads with
//! a per-core priority scheduler, and the thread SVCs create, start, end, put to sleep,
//! reprioritize and move them. Each thread has a TLS block that `TPIDRRO_EL0` points to.
//! Preemption follows ADR-0032: only each core's sliced priority rotates, and a selection
//! change interrupts the affected core at its next block boundary.
//!
//! # Threads
//! Dispatch runs on the guest core that executed the `SVC` instruction, under the kernel lock.
//! [`run_cores`] owns the six core threads (ADR-0017 B2 scope) and joins them before it
//! returns; the crate creates no other thread.
//! It allocates only for the debug-output buffer and the heap bookkeeping, performs no I/O and
//! records unknown SVCs with the non-blocking `lince-trace` recorder.
//!
//! # References
//! ADR-0024 · ADR-0027 · docs/research/nro-and-b1-svcs.md · docs/conventions/patterns.md PAT-01
#![forbid(unsafe_code)]

mod cores;
mod error;
mod kernel;
mod svc;
mod thread;
mod thread_svc;

pub use cores::{Finish, Ticker, run_cores, run_cores_with};
pub use error::{KernelError, Result, RunError};
pub use kernel::{DEFAULT_QUANTUM, Flow, HEAP_BASE, Kernel, Regs};
pub use svc::{ResultCode, SvcNumber, svc_name};
pub use thread::{
    CORE_COUNT, CURRENT_THREAD, DEFAULT_SLICED_PRIORITIES, LOWEST_PRIORITY, TLS_BASE, TLS_SIZE,
    ThreadParams,
};
