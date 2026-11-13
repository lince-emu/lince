// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared helpers for the integration tests.
#![allow(
    dead_code,
    reason = "each test crate uses a different subset of the helpers"
)]

use std::path::PathBuf;

use poc_cache_store::BlobStore;
use poc_cache_store::workload::{self, SizeDist, Workload};

/// A fresh, empty directory unique to this process and test name.
pub(crate) fn fresh_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lince-poc6-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The first `n` keys of a workload.
pub(crate) fn keys(seed: u64, dist: SizeDist, n: usize) -> Vec<u128> {
    let mut w = Workload::new(seed, dist);
    (0..n).map(|_| w.next_key()).collect()
}

/// Puts every key's payload and flushes.
pub(crate) fn fill(store: &mut dyn BlobStore, keys: &[u128]) {
    for &k in keys {
        store.put(k, 1, &workload::payload(k)).unwrap();
    }
    store.flush().unwrap();
}

/// Checks that every lookup is a miss or the exact payload; returns the number of hits.
/// Wrong data is the one outcome PoC-6 must never produce.
pub(crate) fn hits_never_wrong(store: &mut dyn BlobStore, keys: &[u128]) -> usize {
    keys.iter()
        .filter(|&&k| match store.get(k).unwrap() {
            Some(blob) => {
                assert!(workload::verify(k, &blob), "wrong data for key {k:#x}");
                true
            }
            None => false,
        })
        .count()
}
