// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `elf2nro` (PAT-01).

use thiserror::Error;

/// Why an ELF file cannot be converted.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConvertError {
    /// The input is not a little-endian 64-bit `AArch64` ELF, or a table runs past the file.
    #[error("not a usable AArch64 ELF64 file: {0}")]
    BadElf(&'static str),
    /// The load segments do not follow the `nro.ld` layout.
    #[error("unexpected segment layout: {0}")]
    BadLayout(&'static str),
    /// The image is too large for the 32-bit NRO header fields.
    #[error("image does not fit 32-bit NRO offsets")]
    TooLarge,
}

/// `Result` with [`ConvertError`] as the default error type.
pub type Result<T, E = ConvertError> = std::result::Result<T, E>;
