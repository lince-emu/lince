// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Converts a statically linked `AArch64` ELF into an NRO file.
//!
//! # Role
//! Developer tool for the B1 test programs in `tests/homebrew/` (T-029). It reads the ELF load
//! segments laid out by `tests/homebrew/nro.ld` (text at 0, then page-aligned ro and data) and
//! writes the NRO layout of docs/research/nro-and-b1-svcs.md N1-N4: segments padded to whole
//! pages, the `NRO0` header written into the 0x80 zero bytes the start block reserves, no asset
//! block, no MOD0 and no build id. The output depends only on the input, so rebuilding gives
//! identical bytes.
//!
//! # References
//! docs/research/nro-and-b1-svcs.md · ELF-64 object file format (public specification)

mod elf;
mod error;
mod nro;

pub use error::{ConvertError, Result};
pub use nro::convert;
