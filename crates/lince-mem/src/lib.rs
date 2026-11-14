// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The guest address space: mapped regions, permissions, and later fastmem and page tracking.
//!
//! # Role
//! B1 scope of ADR-0019: a 39-bit guest space (doc 01 E2) of mapped regions with permissions,
//! accessed through typed [`GuestVa`](lince_common::GuestVa) reads and writes (QUAL-20) by
//! software lookup. The space is one contiguous host reservation, and the host page size is an
//! explicit parameter, so native execution on ARM64 hosts with 16 KiB pages stays possible.
//!
//! # Threads
//! An [`AddressSpace`] is used by one thread at a time. Reads and writes take no locks, perform
//! no I/O and do not allocate; `map` and `unmap` call the host OS.
//!
//! # Safety
//! `[unsafe]` crate (docs/04-rust-architecture.md §2, ARCH-24): host memory calls live in
//! `platform`, guest memory copies in `space`; each block has a `// SAFETY:` comment.
//!
//! # References
//! docs/04-rust-architecture.md §2 · ADR-0019 · ADR-0031 · docs/conventions/patterns.md PAT-08
#![allow(unsafe_code)]

mod error;
mod platform;
mod space;

pub use error::{Access, MemError, Result};
pub use space::{AddressSpace, GUEST_PAGE_SIZE, Permission, Scalar, SpaceConfig};
