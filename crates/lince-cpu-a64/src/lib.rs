// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The A64 guest CPU (ADR-0004, ADR-0005), implemented in C++ behind a C header (ADR-0030).
//!
//! # Role
//! Owns the only A64 decoder in Lince: a table of instruction forms (`cpp/src/insns.inc`)
//! from which the decoder is generated at compile time. Every execution tier and every Rust
//! tool decodes through it (ADR-0031), so no two components can disagree on an encoding. A
//! word the table lacks is reported as unknown, never guessed. B1 scope: the forms the B1 test
//! programs use; the interpreter follows (T-042).
//!
//! # Threads
//! Decoding is a pure function of the instruction word: no allocation, locks or I/O, callable
//! from any thread, including guest cores.
//!
//! # References
//! ADR-0004 · ADR-0005 · ADR-0030 · ADR-0031 · docs/04-rust-architecture.md §2

mod decode;
mod ffi;

pub use decode::{Insn, decode};
