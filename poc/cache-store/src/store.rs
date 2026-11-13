// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface every engine implements, and engine selection for the harness.

use std::path::Path;

use crate::error::StoreError;
use crate::{custom, redb_store};

/// A store of immutable blobs addressed by 128-bit keys.
pub trait BlobStore: Send {
    /// Stores `payload` under `key`. A key that is already present is left unchanged
    /// (artifacts are content-addressed and immutable).
    fn put(&mut self, key: u128, kind: u8, payload: &[u8]) -> Result<(), StoreError>;

    /// Returns the payload stored under `key`, or `None` on a miss. Corrupt data is a miss.
    fn get(&mut self, key: u128) -> Result<Option<Vec<u8>>, StoreError>;

    /// Makes every completed `put` durable.
    fn flush(&mut self) -> Result<(), StoreError>;

    /// Opens a second, read-only handle that another thread of this process can use.
    fn reader(&self) -> Result<Box<dyn BlobStore>, StoreError>;
}

/// The engines the harness can compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// Segments + memory-mapped index (the design under test).
    Custom,
    /// `redb` embedded database.
    Redb,
    /// SQLite in WAL mode (only with the `sqlite` feature).
    Sqlite,
}

impl Engine {
    /// Parses an engine name as used on the harness command line.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "custom" => Some(Self::Custom),
            "redb" => Some(Self::Redb),
            "sqlite" => Some(Self::Sqlite),
            _ => None,
        }
    }

    /// How a reader in another process shares the store with the writer (CSV `sharing`).
    pub fn sharing(self) -> &'static str {
        match self {
            Self::Custom => "mapped_read_only",
            Self::Redb => "redb_single_writer",
            Self::Sqlite => "wal_query_only",
        }
    }

    /// The command-line name of the engine.
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::Redb => "redb",
            Self::Sqlite => "sqlite",
        }
    }
}

/// Opens (creating if needed) a writable store of the given engine in `dir`.
pub fn open_store(engine: Engine, dir: &Path) -> Result<Box<dyn BlobStore>, StoreError> {
    match engine {
        Engine::Custom => Ok(Box::new(custom::CustomStore::open_writer(dir)?)),
        Engine::Redb => Ok(Box::new(redb_store::RedbStore::open(dir)?)),
        Engine::Sqlite => open_sqlite(dir),
    }
}

/// Opens a read-only handle on an existing store in `dir`, for a reader in another process.
pub fn open_reader(engine: Engine, dir: &Path) -> Result<Box<dyn BlobStore>, StoreError> {
    match engine {
        Engine::Custom => Ok(Box::new(custom::CustomStore::open_read_only(dir)?)),
        Engine::Redb => Ok(Box::new(redb_store::RedbStore::open_read_only(dir)?)),
        Engine::Sqlite => open_sqlite_reader(dir),
    }
}

#[cfg(feature = "sqlite")]
fn open_sqlite_reader(dir: &Path) -> Result<Box<dyn BlobStore>, StoreError> {
    Ok(Box::new(crate::sqlite_store::SqliteStore::open_read_only(
        dir,
    )?))
}

#[cfg(not(feature = "sqlite"))]
fn open_sqlite_reader(dir: &Path) -> Result<Box<dyn BlobStore>, StoreError> {
    open_sqlite(dir)
}

#[cfg(feature = "sqlite")]
fn open_sqlite(dir: &Path) -> Result<Box<dyn BlobStore>, StoreError> {
    Ok(Box::new(crate::sqlite_store::SqliteStore::open(dir)?))
}

#[cfg(not(feature = "sqlite"))]
fn open_sqlite(_dir: &Path) -> Result<Box<dyn BlobStore>, StoreError> {
    Err(StoreError::Engine(
        "built without the `sqlite` feature".to_owned(),
    ))
}
