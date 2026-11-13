// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Behavior every engine must share, plus growth and sealing of the custom store.
#![cfg(test)]

mod support;

use poc_cache_store::custom::{CustomStore, SEGMENT_SEAL_BYTES};
use poc_cache_store::workload::{self, SizeDist};
use poc_cache_store::{BlobStore, Engine, open_store};
use support::{fill, fresh_dir, hits_never_wrong, keys};

const SMALL: SizeDist = SizeDist::Uniform { min: 16, max: 4096 };

fn engines() -> Vec<Engine> {
    let mut all = vec![Engine::Custom, Engine::Redb];
    if cfg!(feature = "sqlite") {
        all.push(Engine::Sqlite);
    }
    all
}

#[test]
fn every_engine_round_trips_across_reopen() {
    for engine in engines() {
        let dir = fresh_dir(&format!("roundtrip-{}", engine.name()));
        let ks = keys(11, SMALL, 200);
        let mut store = open_store(engine, &dir).unwrap();
        fill(&mut *store, &ks);
        assert_eq!(
            hits_never_wrong(&mut *store, &ks),
            ks.len(),
            "{engine:?} before reopen"
        );
        assert_eq!(store.get(0xBAD).unwrap(), None, "{engine:?} unknown key");
        drop(store);

        let mut reopened = open_store(engine, &dir).unwrap();
        assert_eq!(
            hits_never_wrong(&mut *reopened, &ks),
            ks.len(),
            "{engine:?} after reopen"
        );
        let mut reader = reopened.reader().unwrap();
        assert_eq!(
            hits_never_wrong(&mut *reader, &ks),
            ks.len(),
            "{engine:?} second handle"
        );
        drop((reader, reopened));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn putting_an_existing_key_keeps_the_first_payload() {
    for engine in engines() {
        let dir = fresh_dir(&format!("dup-{}", engine.name()));
        let k = keys(5, SMALL, 1)[0];
        let mut store = open_store(engine, &dir).unwrap();
        store.put(k, 1, &workload::payload(k)).unwrap();
        store.put(k, 1, b"different").unwrap();
        store.flush().unwrap();
        assert!(
            workload::verify(k, &store.get(k).unwrap().unwrap()),
            "{engine:?}"
        );
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn custom_index_grows_and_readers_follow_the_new_generation() {
    let dir = fresh_dir("growth");
    let ks = keys(21, SizeDist::Uniform { min: 16, max: 64 }, 3000);
    let mut writer = CustomStore::open_writer(&dir).unwrap();
    fill(&mut writer, &ks[..100]);
    let mut reader = CustomStore::open_read_only(&dir).unwrap();
    assert_eq!(reader.generation(), 1);

    fill(&mut writer, &ks[100..]);
    assert!(
        writer.generation() > 1,
        "3000 entries must outgrow 1024 buckets"
    );
    assert_eq!(hits_never_wrong(&mut writer, &ks), ks.len());
    // The reader still maps generation 1; misses make it pick up the new index.
    assert_eq!(hits_never_wrong(&mut reader, &ks), ks.len());
    assert_eq!(reader.generation(), writer.generation());
    drop((writer, reader));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn custom_store_seals_full_segments() {
    let dir = fresh_dir("seal");
    let one_mib = SizeDist::Uniform {
        min: 1 << 20,
        max: 1 << 20,
    };
    let count = usize::try_from(SEGMENT_SEAL_BYTES >> 20).unwrap() + 4;
    let ks = keys(31, one_mib, count);
    let mut store = CustomStore::open_writer(&dir).unwrap();
    fill(&mut store, &ks);
    let segments = std::fs::read_dir(dir.join("seg")).unwrap().count();
    assert_eq!(segments, 2);
    drop(store);
    let mut reopened = CustomStore::open_read_only(&dir).unwrap();
    assert_eq!(hits_never_wrong(&mut reopened, &ks), ks.len());
    drop(reopened);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn custom_store_stays_writable_after_many_sessions_without_flush() {
    let dir = fresh_dir("noflush");
    let ks = keys(61, SizeDist::Uniform { min: 16, max: 64 }, 1500);
    for session in ks.chunks(50) {
        let mut store = CustomStore::open_writer(&dir).unwrap();
        for &k in session {
            store.put(k, 1, &workload::payload(k)).unwrap();
        }
        // Dropped without flush, like a crash after the writes reached the OS.
    }
    let mut store = CustomStore::open_writer(&dir).unwrap();
    assert!(
        store.generation() > 1,
        "1500 entries must outgrow 1024 buckets"
    );
    assert_eq!(hits_never_wrong(&mut store, &ks), ks.len());
    drop(store);
    std::fs::remove_dir_all(&dir).unwrap();
}
