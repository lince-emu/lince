// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! PoC-6: persistent cache store prototype (docs/06-poc-roadmap.md, ADR-0015, ADR-0016).
//!
//! Three engines behind one [`BlobStore`] interface so the harness can compare them:
//! - [`custom::CustomStore`]: append-only segments plus a memory-mapped open-addressing index
//!   (docs/02-cache-architecture.md §4);
//! - [`redb_store::RedbStore`]: `redb` comparator;
//! - `sqlite_store::SqliteStore`: SQLite (WAL) comparator, behind the `sqlite` feature.
//!
//! This is throwaway prototype code. The property under test — corruption is always a miss,
//! never wrong data — must hold.

pub mod checksum;
mod crc32c;
pub mod custom;
mod error;
mod index;
mod platform;
mod record;
pub mod redb_store;
#[cfg(feature = "sqlite")]
pub mod sqlite_store;
mod store;
pub mod workload;

pub use error::StoreError;
pub use store::{BlobStore, Engine, open_store};
