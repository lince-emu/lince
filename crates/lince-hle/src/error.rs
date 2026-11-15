// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-hle` (PAT-01).

use thiserror::Error;

/// Why a message could not be read or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum IpcError {
    /// The parts of the message do not fit in the message buffer.
    #[error("the message needs {needed:#x} bytes, more than the buffer holds")]
    TooLarge {
        /// Bytes the message would need.
        needed: usize,
    },
    /// A count or value does not fit in its header field.
    #[error("{field} does not fit in its header field")]
    FieldOverflow {
        /// The field.
        field: &'static str,
    },
    /// The raw data is not a whole number of 32-bit words.
    #[error("raw data of {len} bytes is not a whole number of words")]
    UnalignedRaw {
        /// Its length in bytes.
        len: usize,
    },
    /// The raw data is shorter than the header it must start with.
    #[error("raw data of {len} bytes is too short for its header")]
    TooShort {
        /// Its length in bytes.
        len: usize,
    },
    /// The raw data has no CMIF header with the expected signature.
    #[error("the raw data has no CMIF header with signature {expected:#010x}")]
    BadCmifHeader {
        /// The signature expected.
        expected: u32,
    },
    /// The message type is not a TIPC request.
    #[error("message type {tag} is not a TIPC request")]
    NotTipcRequest {
        /// The message type.
        tag: u16,
    },
}

/// `Result` with [`IpcError`] as the default error type.
pub type Result<T, E = IpcError> = std::result::Result<T, E>;
