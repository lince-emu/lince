// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! HLE services and the IPC message framework they speak (ADR-0024).
//!
//! # Role
//! B2 scope, first part (T-053): reading and writing the 0x100-byte IPC message a guest thread
//! keeps at the start of its TLS block. [`hipc`] is the outer layout (header, special header,
//! handles, buffer descriptors, raw data, receive list); [`cmif`] is the request and reply
//! header inside the raw data; [`tipc`] is the CMIF-less variant `sm:` uses. CMIF domains are
//! out of B2 scope (ADR-0024: each session serves one object). [`sm`] is the service manager,
//! the first service a program reaches; [`sm::install`] registers it with the kernel.
//! [`stubs`] holds the first services behind it, the ones the B2 program calls (T-054).
//!
//! Layouts come from docs/research/b2-svcs-and-ipc.md C1–C9 and T1–T3 (Switch 1 knowledge;
//! whether Switch 2 keeps them is UNKNOWN, doc 08 §4). Guessed fields say GUESS.
//!
//! # Threads
//! The message functions are pure over a caller-owned buffer: no threads, I/O or allocation.
//! Session objects run on the guest core that sent the request, under the kernel lock; they
//! allocate only when a session opens (one boxed object) and perform no I/O.
//!
//! # References
//! ADR-0024 · docs/research/b2-svcs-and-ipc.md · docs/conventions/patterns.md PAT-01
#![forbid(unsafe_code)]

pub mod cmif;
mod error;
pub mod hipc;
pub mod sm;
pub mod stubs;
#[cfg(test)]
mod testing;
pub mod tipc;

pub use error::{IpcError, Result};
