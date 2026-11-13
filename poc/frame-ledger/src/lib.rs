// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-7: always-on frame ledger prototype (docs/05-performance-validation.md §3–§4,
//! ADR-0027).
//!
//! - [`Ledger`] hands each thread a [`Recorder`] that writes fixed-size [`Event`]s into its own
//!   lock-free ring. Recording never blocks and never allocates; when a ring is full the oldest
//!   events are overwritten.
//! - [`Ledger::dump`] snapshots every ring while recording continues; [`Dump`] has a versioned
//!   binary format.
//! - [`analyze`] computes frame-time percentiles and attributes each spike to the emulator
//!   events that cover it on render-critical threads.

mod analyzer;
mod dump;
mod event;
mod ledger;
mod ring;

pub use analyzer::{FrameStats, Report, Spike, analyze};
pub use dump::{Dump, DumpError, ThreadDump};
pub use event::{Event, EventKind, ThreadRole};
pub use ledger::{Ledger, Recorder};
