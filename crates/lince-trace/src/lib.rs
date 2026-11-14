// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Observability for Lince: the always-on frame ledger, the Tracy bridge and counters.
//!
//! # Role
//! Records events from every thread so problems can be attributed to a cause
//! (docs/05-performance-validation.md §3). B1 scope (ADR-0027): named events — a stub, an
//! unknown SVC — recorded once per name into per-thread buffers that a non-critical thread
//! drains. Records use the ledger's event shape, so the ledger promoted from PoC-7 later reads
//! the same data.
//!
//! # Threads
//! [`Recorder`] runs on any thread, render-critical ones included: it never blocks, allocates
//! or performs I/O. [`Trace`] (name registration, draining) runs off render-critical threads.
//!
//! # References
//! docs/04-rust-architecture.md §2, §8 · ADR-0027 · docs/conventions/patterns.md PAT-07
#![forbid(unsafe_code)]

mod error;
mod event;
mod trace;

pub use error::{Result, TraceError};
pub use event::{Event, EventKind, ThreadRole};
pub use trace::{Drained, NameId, Record, Recorder, Trace};
