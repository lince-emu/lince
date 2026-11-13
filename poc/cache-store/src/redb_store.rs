// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `redb` comparator. One table `u128 → bytes`; each `put` commits with `Durability::None`
//! and `flush` commits an empty transaction with `Durability::Immediate`, which matches the
//! custom store's "append now, fsync in batches" policy.

use std::path::Path;
use std::sync::Arc;

use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};

use crate::error::StoreError;
use crate::store::BlobStore;

const BLOBS: TableDefinition<'_, u128, &[u8]> = TableDefinition::new("blobs");

fn engine<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Engine(e.to_string())
}

/// A `redb` database file `blobs.redb` in the store directory.
pub struct RedbStore {
    db: Arc<Database>,
}

impl RedbStore {
    /// Opens or creates the database in `dir`.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir).map_err(StoreError::io("create store directory"))?;
        let path = dir.join("blobs.redb");
        let fresh = !path.exists();
        let db = Database::create(&path).map_err(engine)?;
        if fresh {
            // Create the table once, so reopening (what PoC-6 times) does no write.
            let txn = db.begin_write().map_err(engine)?;
            txn.open_table(BLOBS).map_err(engine)?;
            txn.commit().map_err(engine)?;
        }
        Ok(Self { db: Arc::new(db) })
    }

    fn commit_with(
        &self,
        durability: Durability,
        entry: Option<(u128, &[u8])>,
    ) -> Result<(), StoreError> {
        let mut txn = self.db.begin_write().map_err(engine)?;
        txn.set_durability(durability).map_err(engine)?;
        if let Some((key, payload)) = entry {
            let mut table = txn.open_table(BLOBS).map_err(engine)?;
            if table.get(key).map_err(engine)?.is_none() {
                table.insert(key, payload).map_err(engine)?;
            }
        }
        txn.commit().map_err(engine)
    }
}

impl BlobStore for RedbStore {
    fn put(&mut self, key: u128, _kind: u8, payload: &[u8]) -> Result<(), StoreError> {
        self.commit_with(Durability::None, Some((key, payload)))
    }

    fn get(&mut self, key: u128) -> Result<Option<Vec<u8>>, StoreError> {
        let txn = self.db.begin_read().map_err(engine)?;
        let table = txn.open_table(BLOBS).map_err(engine)?;
        Ok(table.get(key).map_err(engine)?.map(|v| v.value().to_vec()))
    }

    fn flush(&mut self) -> Result<(), StoreError> {
        self.commit_with(Durability::Immediate, None)
    }

    fn reader(&self) -> Result<Box<dyn BlobStore>, StoreError> {
        Ok(Box::new(Self {
            db: Arc::clone(&self.db),
        }))
    }
}
