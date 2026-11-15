// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Thread roles and the queues that feed them.
//!
//! # Role
//! `lince-sched` is the only crate that creates host threads for background work (AGENTS.md
//! invariant 7). B2 scope (ADR-0017, ADR-0024): one service-I/O role thread, [`ServiceIo`],
//! fed by a bounded queue, for the blocking service calls that must never run on a guest core
//! thread. Worker pools, priority classes and the governor come later.
//!
//! This crate sits below every subsystem, so it never names a kernel object: completion and
//! "space available" are reported through a caller-supplied [`Signal`], which the kernel
//! implements with its events.
//!
//! # Threads
//! [`ServiceIo::try_submit`] runs on any thread, guest cores included: it never blocks, and a
//! full queue hands the request back instead of waiting or dropping it. Requests run on the
//! service-I/O thread. [`ServiceIo::shutdown`] runs at teardown only.
//!
//! # References
//! docs/04-rust-architecture.md §3 · ADR-0017 · ADR-0024 · docs/conventions/patterns.md PAT-04
#![forbid(unsafe_code)]

mod error;
mod service_io;

pub use error::{Result, SchedError, SubmitError};
pub use service_io::{Request, ServiceIo, Shutdown, Signal};
