// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-trace` (PAT-01).

use thiserror::Error;

/// Errors from setting up a [`Trace`](crate::Trace).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TraceError {
    /// More names were registered than a [`NameId`](crate::NameId) can number.
    #[error("the name table is full ({registered} names)")]
    TooManyNames {
        /// Names already registered.
        registered: usize,
    },
}

/// `Result` with [`TraceError`] as the default error type.
pub type Result<T, E = TraceError> = std::result::Result<T, E>;
