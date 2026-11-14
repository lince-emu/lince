// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-loader` (PAT-01).

use lince_mem::MemError;
use thiserror::Error;

/// Errors from parsing and loading a program image.
#[derive(Debug, Error)]
pub enum LoaderError {
    /// The file ends before the header or before the size its header declares.
    #[error("file is {len:#x} bytes, needs at least {needed:#x}")]
    Truncated {
        /// Bytes required.
        needed: u64,
        /// Bytes present.
        len: u64,
    },
    /// The header magic is not `NRO0`.
    #[error("bad magic {found:02x?}")]
    BadMagic {
        /// The four bytes found.
        found: [u8; 4],
    },
    /// The header sets flags this loader does not handle (for example Compress).
    #[error("unsupported header flags {raw:#x}")]
    UnsupportedFlags {
        /// The raw flags word.
        raw: u32,
    },
    /// A segment is invalid: misaligned, past the file, out of order or empty when required.
    #[error("{segment} segment {offset:#x}+{size:#x} is invalid: {reason}")]
    BadSegment {
        /// Which segment.
        segment: &'static str,
        /// Its file offset.
        offset: u32,
        /// Its size.
        size: u32,
        /// What is wrong.
        reason: &'static str,
    },
    /// The image does not fit at the requested base address.
    #[error("image of {size:#x} bytes does not fit at {base:#x}")]
    BadBase {
        /// The requested base address.
        base: u64,
        /// The image size including bss.
        size: u64,
    },
    /// Mapping or writing guest memory failed.
    #[error("guest memory: {0}")]
    Mem(#[from] MemError),
}

/// `Result` with [`LoaderError`] as the default error type.
pub type Result<T, E = LoaderError> = std::result::Result<T, E>;
