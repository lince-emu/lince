// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-kernel` (PAT-01).
//!
//! Failures a guest can observe (bad arguments, unmapped memory) are SVC result codes, not
//! these errors. These errors mean the dispatcher itself cannot complete the call.

use lince_trace::TraceError;
use thiserror::Error;

/// Errors from creating a [`Kernel`](crate::Kernel) or dispatching an SVC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum KernelError {
    /// The SVC number has no entry in the table.
    #[error("unknown SVC {number:#04x}")]
    UnknownSvc {
        /// The SVC number.
        number: u32,
    },
    /// The SVC is named, but this build has no behavior for it or for the arguments given.
    #[error("SVC {number:#04x} ({name}) is not implemented")]
    Unimplemented {
        /// The SVC number.
        number: u32,
        /// Its public or descriptive name.
        name: &'static str,
    },
    /// The names of the kernel's stubs could not be registered.
    #[error("cannot register kernel trace names: {0}")]
    Trace(#[from] TraceError),
}

/// `Result` with [`KernelError`] as the default error type.
pub type Result<T, E = KernelError> = std::result::Result<T, E>;
