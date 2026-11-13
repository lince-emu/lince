// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Observability for Lince: the always-on frame ledger, the Tracy bridge and counters.
//!
//! # Role
//! Records per-frame events from critical threads so every spike can be attributed to a
//! cause (docs/05-performance-validation.md §3).
//!
//! # Status
//! Empty. The ledger design is prototyped in `poc/frame-ledger` (PoC-7).
//!
//! # References
//! docs/04-rust-architecture.md §2, §8 · ADR-0027
#![forbid(unsafe_code)]
