// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! SQLite comparator (WAL mode, `synchronous = NORMAL`), behind the `sqlite` feature.
//! Bundled SQLite is C code: PoC-only, never part of a release build (ADR-0026).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::StoreError;
use crate::store::BlobStore;

fn engine<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Engine(e.to_string())
}

/// A SQLite database `blobs.sqlite` in the store directory.
pub struct SqliteStore {
    conn: Connection,
    path: PathBuf,
}

impl SqliteStore {
    /// Opens or creates the database in `dir`.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir).map_err(StoreError::io("create store directory"))?;
        let path = dir.join("blobs.sqlite");
        let conn = Self::connect(&path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS blobs (key BLOB PRIMARY KEY, kind INTEGER, payload BLOB) WITHOUT ROWID",
            [],
        )
        .map_err(engine)?;
        Ok(Self { conn, path })
    }

    /// Opens the existing database in `dir` for reading only (`query_only`), as a reader in
    /// another process would while the writer appends in WAL mode.
    pub fn open_read_only(dir: &Path) -> Result<Self, StoreError> {
        let path = dir.join("blobs.sqlite");
        let conn = Self::connect(&path)?;
        conn.pragma_update(None, "query_only", "ON")
            .map_err(engine)?;
        Ok(Self { conn, path })
    }

    fn connect(path: &Path) -> Result<Connection, StoreError> {
        let conn = Connection::open(path).map_err(engine)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(engine)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(engine)?;
        // Force the schema to be read so "open" includes the first metadata access.
        conn.query_row("PRAGMA schema_version", [], |_| Ok(()))
            .map_err(engine)?;
        Ok(conn)
    }
}

impl BlobStore for SqliteStore {
    fn put(&mut self, key: u128, kind: u8, payload: &[u8]) -> Result<(), StoreError> {
        self.conn
            .prepare_cached("INSERT OR IGNORE INTO blobs (key, kind, payload) VALUES (?1, ?2, ?3)")
            .and_then(|mut s| s.execute(params![key.to_be_bytes(), kind, payload]))
            .map_err(engine)?;
        Ok(())
    }

    fn get(&mut self, key: u128) -> Result<Option<Vec<u8>>, StoreError> {
        self.conn
            .prepare_cached("SELECT payload FROM blobs WHERE key = ?1")
            .and_then(|mut s| {
                s.query_row(params![key.to_be_bytes()], |row| row.get(0))
                    .optional()
            })
            .map_err(engine)
    }

    fn flush(&mut self) -> Result<(), StoreError> {
        self.conn
            .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()))
            .map_err(engine)
    }

    fn reader(&self) -> Result<Box<dyn BlobStore>, StoreError> {
        Ok(Box::new(Self {
            conn: Self::connect(&self.path)?,
            path: self.path.clone(),
        }))
    }
}
