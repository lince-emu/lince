// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Memory-mapped open-addressing hash index (docs/02-cache-architecture.md §4.2).
//!
//! Power-of-two bucket count, linear probing, 32-byte entries of four little-endian `u64`
//! words, accessed only through atomics so a writer and readers (in this or another process)
//! can share the mapping:
//!
//! ```text
//! w0 key bits 0..64     w2 segment u32 | offset (×16 bytes) u32 << 32
//! w1 key bits 64..128   w3 len u32 | kind u8 << 32 | flags u8 << 40 | entry_check u16 << 48
//! ```
//!
//! The writer stores `w0..w2` first and publishes `w3` last with `Release`; readers load `w3`
//! with `Relaxed` followed by an `Acquire` fence. `entry_check` is never zero, so `w3 == 0`
//! means an empty bucket, and a torn or bit-flipped entry fails the check and is skipped (a
//! miss, never wrong data).
//!
//! The crate's only `unsafe` code lives here (maintainer-approved policy for prototype
//! crates: `unsafe` allowed when confined and documented with `// SAFETY:` comments).
#![allow(unsafe_code, reason = "atomic view of the mapped index; see words()")]

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering, fence};

use memmap2::{MmapOptions, MmapRaw};

use crate::crc32c;
use crate::error::StoreError;

/// Bytes per index entry.
pub(crate) const ENTRY_LEN: u64 = 32;
const WORDS_PER_ENTRY: usize = 4;
/// Fibonacci-hashing multiplier (2^64 / golden ratio).
const HASH_MUL: u64 = 0x9E37_79B9_7F4A_7C15;

/// Where a record lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Location {
    pub(crate) segment: u32,
    /// Offset in the segment, in 16-byte units.
    pub(crate) offset_units: u32,
    pub(crate) len: u32,
    pub(crate) kind: u8,
}

/// Result of an insert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Insert {
    Inserted,
    AlreadyPresent,
    Full,
}

/// Contents of one bucket.
enum Slot {
    Empty,
    /// Fails its `entry_check`: torn or corrupt; skipped.
    Damaged,
    Valid(u128, Location),
}

/// A mapped index file.
pub(crate) struct MappedIndex {
    map: MmapRaw,
    log2_buckets: u32,
    writable: bool,
}

impl MappedIndex {
    /// Creates a zeroed index file with `2^log2_buckets` buckets and maps it writable.
    pub(crate) fn create(path: &Path, log2_buckets: u32) -> Result<Self, StoreError> {
        let len = index_len(log2_buckets)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(StoreError::io("create index"))?;
        file.set_len(len).map_err(StoreError::io("size index"))?;
        Self::map(&file, log2_buckets, true)
    }

    /// Maps an existing index file, writable or read-only.
    pub(crate) fn open(path: &Path, log2_buckets: u32, writable: bool) -> Result<Self, StoreError> {
        let expected = index_len(log2_buckets)?;
        let file = OpenOptions::new()
            .read(true)
            .write(writable)
            .open(path)
            .map_err(StoreError::io("open index"))?;
        let len = file.metadata().map_err(StoreError::io("stat index"))?.len();
        if len != expected {
            return Err(StoreError::BadMetadata(
                "index size does not match the manifest",
            ));
        }
        Self::map(&file, log2_buckets, writable)
    }

    fn map(file: &File, log2_buckets: u32, writable: bool) -> Result<Self, StoreError> {
        let expected = usize::try_from(index_len(log2_buckets)?)
            .map_err(|_| StoreError::BadMetadata("index too large for this host"))?;
        let map = if writable {
            MmapOptions::new().map_raw(file)
        } else {
            MmapOptions::new().map_raw_read_only(file)
        }
        .map_err(StoreError::io("map index"))?;
        if map.as_ptr().align_offset(8) != 0 || map.len() != expected {
            return Err(StoreError::BadMetadata(
                "index mapping misaligned or of the wrong length",
            ));
        }
        Ok(Self {
            map,
            log2_buckets,
            writable,
        })
    }

    /// Number of buckets.
    pub(crate) fn buckets(&self) -> u64 {
        1 << self.log2_buckets
    }

    pub(crate) fn log2_buckets(&self) -> u32 {
        self.log2_buckets
    }

    fn words(&self) -> &[AtomicU64] {
        // SAFETY: the pointer and length describe the live mapping owned by `self`, so the
        // slice cannot outlive it; the store never truncates a mapped index file. `AtomicU8`
        // has the size and alignment of `u8`, every byte value is valid, and all accesses go
        // through atomics, so concurrent writes by the single writer (in this or another
        // process) are not data races.
        let bytes = unsafe {
            std::slice::from_raw_parts(self.map.as_ptr().cast::<AtomicU8>(), self.map.len())
        };
        // SAFETY: eight `AtomicU8` have the size and bit validity of one `AtomicU64`, both are
        // interior-mutable, and only the `AtomicU64` view is ever accessed. `map` checked that
        // the mapping is 8-byte aligned and its length a multiple of 32, so the whole mapping
        // lands in the aligned middle slice. Readers of a read-only mapping only perform
        // 8-byte `Relaxed` loads, the one kind of atomic access to read-only memory that
        // `core::sync::atomic` ("Atomic accesses to read-only memory") permits; `Acquire`
        // ordering comes from a separate fence. Stores happen only on writable mappings.
        let (head, words, tail) = unsafe { bytes.align_to::<AtomicU64>() };
        debug_assert!(head.is_empty() && tail.is_empty());
        words
    }

    fn start_bucket(&self, key: u128) -> u64 {
        let [lo, hi] = split(key);
        (lo ^ hi).wrapping_mul(HASH_MUL) >> (64 - self.log2_buckets)
    }

    fn entry(&self, bucket: u64) -> &[AtomicU64] {
        let first = usize::try_from(bucket).unwrap_or(usize::MAX) * WORDS_PER_ENTRY;
        &self.words()[first..first + WORDS_PER_ENTRY]
    }

    fn read(&self, bucket: u64) -> Slot {
        let e = self.entry(bucket);
        let w3 = e[3].load(Ordering::Relaxed);
        if w3 == 0 {
            return Slot::Empty;
        }
        // Pairs with the writer's Release store of w3: w0..w2 are visible below.
        fence(Ordering::Acquire);
        let words = [
            e[0].load(Ordering::Relaxed),
            e[1].load(Ordering::Relaxed),
            e[2].load(Ordering::Relaxed),
            w3,
        ];
        decode(words).map_or(Slot::Damaged, |(k, loc)| Slot::Valid(k, loc))
    }

    /// Looks up `key`. Damaged entries are skipped.
    pub(crate) fn lookup(&self, key: u128) -> Option<Location> {
        let mask = self.buckets() - 1;
        let start = self.start_bucket(key);
        for i in 0..self.buckets() {
            match self.read((start + i) & mask) {
                Slot::Empty => return None,
                Slot::Valid(k, loc) if k == key => return Some(loc),
                Slot::Valid(..) | Slot::Damaged => {}
            }
        }
        None
    }

    /// Inserts `key`. Only the single writer calls this, on a writable mapping.
    pub(crate) fn insert(&self, key: u128, loc: Location) -> Result<Insert, StoreError> {
        if !self.writable {
            return Err(StoreError::BadMetadata("insert into a read-only index"));
        }
        let mask = self.buckets() - 1;
        let start = self.start_bucket(key);
        for i in 0..self.buckets() {
            let bucket = (start + i) & mask;
            match self.read(bucket) {
                Slot::Empty => {
                    let words = encode(key, loc);
                    let e = self.entry(bucket);
                    e[0].store(words[0], Ordering::Relaxed);
                    e[1].store(words[1], Ordering::Relaxed);
                    e[2].store(words[2], Ordering::Relaxed);
                    e[3].store(words[3], Ordering::Release);
                    return Ok(Insert::Inserted);
                }
                Slot::Valid(k, _) if k == key => return Ok(Insert::AlreadyPresent),
                Slot::Valid(..) | Slot::Damaged => {}
            }
        }
        Ok(Insert::Full)
    }

    /// Every valid entry, in bucket order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (u128, Location)> + '_ {
        (0..self.buckets()).filter_map(|b| match self.read(b) {
            Slot::Valid(k, loc) => Some((k, loc)),
            Slot::Empty | Slot::Damaged => None,
        })
    }

    /// Writes dirty pages back to the file.
    pub(crate) fn flush(&self) -> Result<(), StoreError> {
        self.map.flush().map_err(StoreError::io("flush index"))
    }
}

/// Index file length for `2^log2_buckets` buckets, after checking the range.
fn index_len(log2_buckets: u32) -> Result<u64, StoreError> {
    if (4..=40).contains(&log2_buckets) {
        Ok(ENTRY_LEN << log2_buckets)
    } else {
        Err(StoreError::BadMetadata("index bucket count out of range"))
    }
}

fn split(key: u128) -> [u64; 2] {
    let b = key.to_le_bytes();
    let mut lo = [0u8; 8];
    let mut hi = [0u8; 8];
    lo.copy_from_slice(&b[..8]);
    hi.copy_from_slice(&b[8..]);
    [u64::from_le_bytes(lo), u64::from_le_bytes(hi)]
}

fn entry_check(words: [u64; 4]) -> u16 {
    let mut bytes = [0u8; 30];
    bytes[..8].copy_from_slice(&words[0].to_le_bytes());
    bytes[8..16].copy_from_slice(&words[1].to_le_bytes());
    bytes[16..24].copy_from_slice(&words[2].to_le_bytes());
    bytes[24..].copy_from_slice(&words[3].to_le_bytes()[..6]);
    let [a, b, c, d] = crc32c::checksum(&bytes).to_le_bytes();
    let folded = u16::from_le_bytes([a ^ c, b ^ d]);
    if folded == 0 { 1 } else { folded }
}

fn encode(key: u128, loc: Location) -> [u64; 4] {
    let [lo, hi] = split(key);
    let w2 = u64::from(loc.segment) | (u64::from(loc.offset_units) << 32);
    let w3 = u64::from(loc.len) | (u64::from(loc.kind) << 32);
    let mut words = [lo, hi, w2, w3];
    words[3] |= u64::from(entry_check(words)) << 48;
    words
}

fn decode(words: [u64; 4]) -> Option<(u128, Location)> {
    let stored = words[3].to_le_bytes();
    let check = u16::from_le_bytes([stored[6], stored[7]]);
    let mut unchecked = words;
    unchecked[3] &= (1 << 48) - 1;
    if stored[5] != 0 || entry_check(unchecked) != check {
        return None;
    }
    let w2 = words[2].to_le_bytes();
    let key = u128::from(words[0]) | (u128::from(words[1]) << 64);
    Some((
        key,
        Location {
            segment: u32::from_le_bytes([w2[0], w2[1], w2[2], w2[3]]),
            offset_units: u32::from_le_bytes([w2[4], w2[5], w2[6], w2[7]]),
            len: u32::from_le_bytes([stored[0], stored[1], stored[2], stored[3]]),
            kind: stored[4],
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{Insert, Location, MappedIndex, decode, encode};

    fn loc(n: u32) -> Location {
        Location {
            segment: 1,
            offset_units: n,
            len: n * 3,
            kind: 2,
        }
    }

    #[test]
    fn entry_encoding_round_trips_and_is_never_empty() {
        let words = encode(0x1234_5678_9ABC_DEF0_0FED_CBA9_8765_4321, loc(7));
        assert_ne!(words[3], 0);
        assert_eq!(
            decode(words),
            Some((0x1234_5678_9ABC_DEF0_0FED_CBA9_8765_4321, loc(7)))
        );
    }

    #[test]
    fn single_bit_flips_never_decode_to_a_different_valid_entry() {
        let key = 0xDEAD_BEEF_u128 << 70 | 0x63;
        let words = encode(key, loc(5));
        for bit in 0..256 {
            let mut bad = words;
            bad[bit / 64] ^= 1 << (bit % 64);
            if let Some(found) = decode(bad) {
                assert_eq!(found, (key, loc(5)), "bit {bit} produced a different entry");
            }
        }
    }

    #[test]
    fn insert_then_lookup_and_fill_to_capacity() {
        let dir = std::env::temp_dir().join(format!("lince-poc6-index-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("index.test");
        let index = MappedIndex::create(&path, 4).unwrap();
        for k in 0..16u32 {
            assert_eq!(
                index.insert(u128::from(k) << 80, loc(k)).unwrap(),
                Insert::Inserted
            );
        }
        assert_eq!(index.insert(1, loc(1)).unwrap(), Insert::Full);
        assert_eq!(index.insert(0, loc(0)).unwrap(), Insert::AlreadyPresent);
        for k in 0..16u32 {
            assert_eq!(index.lookup(u128::from(k) << 80), Some(loc(k)));
        }
        assert_eq!(index.lookup(12345), None);
        assert_eq!(index.entries().count(), 16);
        drop(index);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
