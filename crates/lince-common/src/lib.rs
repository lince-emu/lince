// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared building blocks for every Lince crate.
//!
//! # Role
//! Ids, error helpers, units and small domain-free utilities used by three or more crates
//! (QUAL-60). Nothing here depends on another Lince crate.
//!
//! This crate is also the **reference crate** (backlog T-005): its layout, module docs, error
//! enum (PAT-01), newtypes (PAT-02) and tests (QUAL-70–73) are the model later crates copy.
//!
//! # Threads
//! Plain values: safe on render-critical threads (no allocation, I/O or locks).
//!
//! # References
//! docs/04-rust-architecture.md §2 · ADR-0001 · docs/conventions/patterns.md
#![forbid(unsafe_code)]

mod address;
mod error;
mod time;

pub use address::{GpuVa, GuestVa};
pub use error::{CommonError, Result};
pub use time::Nanos;
