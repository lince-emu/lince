// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Horizon-compatible kernel HLE: the SVC boundary and the kernel state behind it.
//!
//! # Role
//! B1 scope (ADR-0024): a table indexed by SVC number dispatches the memory SVCs, process exit,
//! `Break` (a named stub) and `OutputDebugString`. The Switch 2 SVC `0x80` is reserved by name.
//! An SVC with no handler returns a distinct error and is recorded once through `lince-trace`;
//! it never reports success. Numbers, arguments and results come from
//! docs/research/nro-and-b1-svcs.md (Switch 1 knowledge; Switch 2 values are UNKNOWN).
//!
//! # Threads
//! Dispatch runs on the guest core that executed the `SVC` instruction. B1 has one guest thread.
//! It allocates only for the debug-output buffer and the heap bookkeeping, performs no I/O and
//! records unknown SVCs with the non-blocking `lince-trace` recorder.
//!
//! # References
//! ADR-0024 · ADR-0027 · docs/research/nro-and-b1-svcs.md · docs/conventions/patterns.md PAT-01
#![forbid(unsafe_code)]

mod error;
mod kernel;
mod svc;

pub use error::{KernelError, Result};
pub use kernel::{Flow, HEAP_BASE, Kernel, Regs};
pub use svc::{ResultCode, SvcNumber, svc_name};
