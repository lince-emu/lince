// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Executable loaders: parse a program image and map it into a guest address space.
//!
//! # Role
//! B1 scope: homebrew NRO files (docs/research/nro-and-b1-svcs.md). The header is decoded once
//! into typed fields (PAT-03) and every size and offset is checked against the file before
//! anything is mapped, so a corrupt or truncated file is an error, never a panic.
//!
//! # Threads
//! Loading runs once per program, before any guest thread starts; it is not on a
//! render-critical path. It allocates nothing beyond the guest mappings and performs no I/O:
//! the caller reads the file.
//!
//! # References
//! ADR-0024 · ADR-0028 · docs/research/nro-and-b1-svcs.md · docs/conventions/patterns.md PAT-03

mod error;
mod nro;

pub use error::{LoaderError, Result};
pub use nro::{LoadedNro, NroFlags, NroHeader, Segment, load_nro};
