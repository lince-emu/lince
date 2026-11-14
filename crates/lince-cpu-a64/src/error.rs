// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-cpu-a64` (PAT-01).

use thiserror::Error;

/// Errors crossing the C++ boundary. Each one means the C++ side broke the header's contract,
/// so it is reported, never mapped to a guest-visible result (PAT-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum CpuError {
    /// A pointer argument was null; the wrapper never passes one, so this is a C++ bug.
    #[error("null argument passed to the C++ interpreter")]
    NullArgument,
    /// The address space has more mapped regions than one `run` call can pass to C++.
    #[error("the address space has more than {max} mapped regions")]
    TooManyRegions {
        /// The most regions a call passes.
        max: usize,
    },
    /// A call returned a status this crate does not expect.
    #[error("the C++ interpreter returned status {status}")]
    Status {
        /// The raw `lca_status`.
        status: i32,
    },
    /// `lca_run` returned an exit reason or detail this crate does not know.
    #[error("the C++ interpreter returned exit reason {reason} with detail {detail}")]
    UnknownExit {
        /// The raw `lca_exit_reason`.
        reason: u32,
        /// The raw detail field.
        detail: u32,
    },
}
