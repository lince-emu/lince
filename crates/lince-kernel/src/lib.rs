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
//! A per-process handle table names threads and events (T-052); events are created, signaled,
//! cleared and reset through their SVCs, and `CloseHandle` closes any handle. A thread waits on
//! events and threads with `WaitSynchronization`, with a timeout, until another thread signals
//! one or cancels the wait; only such a wait blocks a guest thread (ADR-0004). Mutexes and
//! condition variables live in guest memory; `ArbitrateLock`/`ArbitrateUnlock` and
//! `WaitProcessWideKeyAtomic`/`SignalProcessWideKey` queue the threads that must block and
//! update the words with host compare-and-swaps.
//! IPC sessions (T-053) are served by host [`SessionObject`]s: `ConnectToNamedPort` opens one
//! from a registered port, and `SendSyncRequest` hands it the thread's TLS message buffer. A
//! command an object does not know is logged once by name and stops the guest.
//! Preemption follows ADR-0032: only each core's sliced priority rotates, and a selection
//! change interrupts the affected core at its next block boundary.
//!
//! # Threads
//! Dispatch runs on the guest core that executed the `SVC` instruction, under the kernel lock.
//! [`run_cores`] owns the six core threads (ADR-0017 B2 scope) and joins them before it
//! returns; the crate creates no other thread.
//! It allocates only for the debug-output buffer, the heap bookkeeping and the bounded thread,
//! handle and event tables, performs no I/O and records unknown SVCs with the non-blocking
//! `lince-trace` recorder.
//!
//! # References
//! ADR-0024 · ADR-0027 · docs/research/nro-and-b1-svcs.md · docs/conventions/patterns.md PAT-01
#![forbid(unsafe_code)]

mod arbiter_svc;
mod cores;
mod error;
mod event_svc;
mod handle;
#[cfg(all(test, loom))]
mod handoff_loom;
mod ipc_svc;
mod kernel;
mod svc;
mod thread;
mod thread_svc;
mod wait_svc;

pub use cores::{Finish, Ticker, run_cores, run_cores_with};
pub use error::{KernelError, Result, RunError};
pub use ipc_svc::{
    IPC_MESSAGE_SIZE, IpcMessage, PortFactory, SessionObject, Sessions, UnknownCommand,
};
pub use kernel::{DEFAULT_QUANTUM, Flow, HEAP_BASE, Kernel, Regs};
pub use svc::{ResultCode, SvcNumber, svc_name};
pub use thread::{
    CORE_COUNT, CURRENT_THREAD, DEFAULT_SLICED_PRIORITIES, LOWEST_PRIORITY, TLS_BASE, TLS_SIZE,
    ThreadParams,
};
