// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The crate's error type (PAT-01).

use std::fmt;

/// Errors from any store engine. Corrupt *data* is never an error: it is a miss.
#[derive(Debug)]
pub enum StoreError {
    /// An I/O operation failed.
    Io {
        /// What the store was doing.
        context: &'static str,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The manifest or index metadata cannot be used.
    BadMetadata(&'static str),
    /// A payload is too large for the record format.
    TooLarge(usize),
    /// A comparator engine reported an error.
    Engine(String),
}

impl StoreError {
    /// Wraps an I/O error with what the store was doing.
    pub(crate) fn io(context: &'static str) -> impl FnOnce(std::io::Error) -> Self {
        move |source| Self::Io { context, source }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::BadMetadata(what) => write!(f, "unusable store metadata: {what}"),
            Self::TooLarge(len) => write!(f, "payload of {len} bytes is too large"),
            Self::Engine(msg) => write!(f, "engine error: {msg}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::BadMetadata(_) | Self::TooLarge(_) | Self::Engine(_) => None,
        }
    }
}
