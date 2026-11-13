// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-11: a C++20 static library behind one C header, built by `cc` and called from Rust
//! (docs/06-poc-roadmap.md, ADR-0030).
//!
//! The crate follows the `[c++]` layout of ADR-0030: `cpp/include/boundary.h` is the contract,
//! `cpp/src/` holds the C++, `src/ffi/bindings.rs` is generated from the header, and
//! `src/ffi.rs` is the safe wrapper and the only module with `unsafe`.

mod ffi;

pub use ffi::{BoundaryError, Memory, add, checked_div, sum_range};
