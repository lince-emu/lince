// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The error enum of `lince-mem` (PAT-01).

use std::fmt;
use std::io;

use lince_common::GuestVa;
use thiserror::Error;

/// The kind of guest access that faulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// A load.
    Read,
    /// A store.
    Write,
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "read",
            Self::Write => "write",
        })
    }
}

/// Errors from building, mapping and accessing an [`AddressSpace`](crate::AddressSpace).
#[derive(Debug, Error)]
pub enum MemError {
    /// The [`SpaceConfig`](crate::SpaceConfig) cannot be used on this host.
    #[error("invalid address-space configuration: {reason}")]
    InvalidConfig {
        /// What is wrong.
        reason: &'static str,
    },
    /// A host virtual-memory call failed.
    #[error("host memory call failed: {0}")]
    Host(#[from] io::Error),
    /// A range is empty or not aligned to guest pages.
    #[error("range {va:#x}+{size:#x} is empty or not page-aligned")]
    Misaligned {
        /// Start of the range.
        va: GuestVa,
        /// Length of the range in bytes.
        size: u64,
    },
    /// A range ends past the address space.
    #[error("range {va:#x}+{size:#x} is outside the address space")]
    OutOfRange {
        /// Start of the range.
        va: GuestVa,
        /// Length of the range in bytes.
        size: u64,
    },
    /// A range overlaps a mapped region.
    #[error("range {va:#x}+{size:#x} overlaps a mapped region")]
    Overlap {
        /// Start of the range.
        va: GuestVa,
        /// Length of the range in bytes.
        size: u64,
    },
    /// An unmap range is not exactly one mapped region.
    #[error("range {va:#x}+{size:#x} is not a mapped region")]
    NotARegion {
        /// Start of the range.
        va: GuestVa,
        /// Length of the range in bytes.
        size: u64,
    },
    /// A guest access touched an unmapped address.
    #[error("{access} of unmapped address {va:#x}")]
    Unmapped {
        /// First unmapped address of the access.
        va: GuestVa,
        /// The access.
        access: Access,
    },
    /// A guest access is not allowed by the region's permission.
    #[error("{access} of {va:#x} is not permitted")]
    PermissionDenied {
        /// First address of the access in the faulting region.
        va: GuestVa,
        /// The access.
        access: Access,
    },
}

/// `Result` with [`MemError`] as the default error type.
pub type Result<T, E = MemError> = std::result::Result<T, E>;
