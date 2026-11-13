// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-common` (PAT-01).

use thiserror::Error;

/// Errors from the conversions in this crate. Every variant carries the value that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum CommonError {
    /// A `Duration` longer than [`Nanos`](crate::Nanos) can hold (about 584 years).
    #[error("duration of {secs} s does not fit in 64-bit nanoseconds")]
    DurationTooLong {
        /// Whole seconds of the rejected duration.
        secs: u64,
    },
}

/// `Result` with [`CommonError`] as the default error type.
pub type Result<T, E = CommonError> = std::result::Result<T, E>;
