// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `redb` comparator. One table `u128 → bytes`.
//!
//! Writer and readers use `ConcurrencyMode::SingleWriter`, so readers in other processes can
//! open the file read-only and follow the writer's commits (doc 02 §5, §10; decision D-002).
//! That mode refuses non-durable commits (`Durability::None`), so `put` buffers artifacts in
//! the writer and `flush` inserts them in one `Durability::Immediate` transaction: the custom
//! store's "append now, fsync in batches" policy, except that readers see a batch only once
//! it is flushed.

use std::path::Path;
use std::sync::Arc;

use redb::{
    Builder, ConcurrencyMode, Database, Durability, ReadOnlyDatabase, ReadableDatabase,
    ReadableTable, TableDefinition,
};

use crate::error::StoreError;
use crate::store::BlobStore;

const BLOBS: TableDefinition<'_, u128, &[u8]> = TableDefinition::new("blobs");

fn engine<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Engine(e.to_string())
}

/// A `redb` database file `blobs.redb` in the store directory.
pub struct RedbStore {
    db: Handle,
    /// Artifacts `put` since the last `flush`, committed by the next `flush`.
    pending: Vec<(u128, Vec<u8>)>,
}

/// The writer's database, or a read-only handle (possibly in another process).
enum Handle {
    Writer(Arc<Database>),
    Reader(ReadOnlyDatabase),
}

fn builder() -> Builder {
    let mut b = Builder::new();
    b.set_concurrency_mode(ConcurrencyMode::SingleWriter);
    b
}

fn read<D: ReadableDatabase>(db: &D, key: u128) -> Result<Option<Vec<u8>>, StoreError> {
    let txn = db.begin_read().map_err(engine)?;
    let table = txn.open_table(BLOBS).map_err(engine)?;
    Ok(table.get(key).map_err(engine)?.map(|v| v.value().to_vec()))
}

/// Inserts each artifact whose key the table does not hold yet (artifacts are immutable).
fn insert_missing(
    table: &mut redb::Table<'_, u128, &[u8]>,
    pending: &[(u128, Vec<u8>)],
) -> Result<(), StoreError> {
    for (key, payload) in pending {
        if table.get(*key).map_err(engine)?.is_none() {
            table.insert(*key, payload.as_slice()).map_err(engine)?;
        }
    }
    Ok(())
}

impl RedbStore {
    /// Opens or creates the database in `dir`.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir).map_err(StoreError::io("create store directory"))?;
        let path = dir.join("blobs.redb");
        let fresh = !path.exists();
        let db = builder().create(&path).map_err(engine)?;
        if fresh {
            // Create the table once, so reopening (what PoC-6 times) does no write.
            let txn = db.begin_write().map_err(engine)?;
            txn.open_table(BLOBS).map_err(engine)?;
            txn.commit().map_err(engine)?;
        }
        Ok(Self {
            db: Handle::Writer(Arc::new(db)),
            pending: Vec::new(),
        })
    }

    /// Opens the existing database in `dir` read-only, in the writer's `SingleWriter` mode, so
    /// it can run in another process while the writer commits.
    pub fn open_read_only(dir: &Path) -> Result<Self, StoreError> {
        let db = builder()
            .open_read_only(dir.join("blobs.redb"))
            .map_err(engine)?;
        Ok(Self {
            db: Handle::Reader(db),
            pending: Vec::new(),
        })
    }

    fn writer(&self) -> Result<&Database, StoreError> {
        match &self.db {
            Handle::Writer(db) => Ok(db),
            Handle::Reader(_) => Err(StoreError::Engine(
                "write through a read-only handle".to_owned(),
            )),
        }
    }

    /// Inserts every pending artifact not already stored, in one durable transaction.
    fn commit_pending(&mut self) -> Result<(), StoreError> {
        let mut txn = self.writer()?.begin_write().map_err(engine)?;
        txn.set_durability(Durability::Immediate).map_err(engine)?;
        insert_missing(&mut txn.open_table(BLOBS).map_err(engine)?, &self.pending)?;
        txn.commit().map_err(engine)?;
        self.pending.clear();
        Ok(())
    }
}

impl BlobStore for RedbStore {
    fn put(&mut self, key: u128, _kind: u8, payload: &[u8]) -> Result<(), StoreError> {
        self.writer()?;
        if !self.pending.iter().any(|(k, _)| *k == key) {
            self.pending.push((key, payload.to_vec()));
        }
        Ok(())
    }

    fn get(&mut self, key: u128) -> Result<Option<Vec<u8>>, StoreError> {
        if let Some((_, payload)) = self.pending.iter().find(|(k, _)| *k == key) {
            return Ok(Some(payload.clone()));
        }
        match &self.db {
            Handle::Writer(db) => read(db.as_ref(), key),
            Handle::Reader(db) => read(db, key),
        }
    }

    fn flush(&mut self) -> Result<(), StoreError> {
        self.commit_pending()
    }

    fn reader(&self) -> Result<Box<dyn BlobStore>, StoreError> {
        match &self.db {
            Handle::Writer(db) => Ok(Box::new(Self {
                db: Handle::Writer(Arc::clone(db)),
                pending: Vec::new(),
            })),
            Handle::Reader(_) => Err(StoreError::Engine(
                "open another reader with `RedbStore::open_read_only`".to_owned(),
            )),
        }
    }
}
