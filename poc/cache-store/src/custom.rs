// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The store under test: append-only segments plus a memory-mapped index
//! (docs/02-cache-architecture.md §4, §5, §7, §9).
//!
//! Layout of a store directory:
//!
//! ```text
//! MANIFEST          32 bytes, replaced atomically (write temp → fsync → rename)
//! index.<gen>       mapped open-addressing table (see `index`)
//! seg/NNNNNN.lseg   append-only segments, sealed at 64 MiB
//! ```
//!
//! Deliberate prototype simplifications (listed in the `PoC-6` report): no `store.lock`, so one writer
//! process is a caller obligation; index growth runs synchronously on the writer; no
//! background tail scan, so records written but not indexed before a crash are lost (a miss);
//! corrupt entries are not tombstoned, so a key whose record is damaged stays a miss until
//! the store is rebuilt.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::error::StoreError;
use crate::index::{Insert, Location, MappedIndex};
use crate::record::{self, ALIGN, HEADER_LEN};
use crate::store::BlobStore;
use crate::{crc32c, platform};

/// Segments are sealed once they would grow past this size.
pub const SEGMENT_SEAL_BYTES: u64 = 64 << 20;
/// A new store starts with `2^INITIAL_LOG2_BUCKETS` index buckets.
pub const INITIAL_LOG2_BUCKETS: u32 = 10;
const MANIFEST_MAGIC: [u8; 4] = *b"LNXM";
/// 2: payload checksums are XXH3-64 (T-020); version-1 stores used CRC-32C.
const MANIFEST_VERSION: u32 = 2;
const MANIFEST_LEN: usize = 32;

/// Contents of the `MANIFEST` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Manifest {
    generation: u32,
    log2_buckets: u32,
    segment_count: u32,
    entry_count: u64,
}

impl Manifest {
    fn encode(self) -> [u8; MANIFEST_LEN] {
        let mut b = [0u8; MANIFEST_LEN];
        b[0..4].copy_from_slice(&MANIFEST_MAGIC);
        b[4..8].copy_from_slice(&MANIFEST_VERSION.to_le_bytes());
        b[8..12].copy_from_slice(&self.generation.to_le_bytes());
        b[12..16].copy_from_slice(&self.log2_buckets.to_le_bytes());
        b[16..20].copy_from_slice(&self.segment_count.to_le_bytes());
        b[20..28].copy_from_slice(&self.entry_count.to_le_bytes());
        let crc = crc32c::checksum(&b[0..28]);
        b[28..32].copy_from_slice(&crc.to_le_bytes());
        b
    }

    fn decode(b: &[u8]) -> Result<Self, StoreError> {
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if b.len() != MANIFEST_LEN || b[0..4] != MANIFEST_MAGIC {
            return Err(StoreError::BadMetadata("manifest magic or size"));
        }
        if crc32c::checksum(&b[0..28]) != u32_at(28) {
            return Err(StoreError::BadMetadata("manifest checksum"));
        }
        if u32_at(4) != MANIFEST_VERSION {
            return Err(StoreError::BadMetadata("manifest version"));
        }
        let mut count = [0u8; 8];
        count.copy_from_slice(&b[20..28]);
        Ok(Self {
            generation: u32_at(8),
            log2_buckets: u32_at(12),
            segment_count: u32_at(16),
            entry_count: u64::from_le_bytes(count),
        })
    }
}

/// The append side of a writable store.
struct Writer {
    active: File,
    active_len: u64,
}

/// Segment store with a memory-mapped index. See the module docs.
pub struct CustomStore {
    root: PathBuf,
    manifest: Manifest,
    index: MappedIndex,
    /// Read handles, indexed by segment number − 1, opened on first use.
    segments: Vec<Option<File>>,
    writer: Option<Writer>,
    corrupt_misses: u64,
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join("MANIFEST")
}

fn index_path(root: &Path, generation: u32) -> PathBuf {
    root.join(format!("index.{generation}"))
}

fn segment_path(root: &Path, segment: u32) -> PathBuf {
    root.join("seg").join(format!("{segment:06}.lseg"))
}

fn read_manifest(root: &Path) -> Result<Manifest, StoreError> {
    let bytes = fs::read(manifest_path(root)).map_err(StoreError::io("read manifest"))?;
    Manifest::decode(&bytes)
}

fn write_manifest(root: &Path, manifest: Manifest) -> Result<(), StoreError> {
    let tmp = root.join("MANIFEST.tmp");
    let mut f = File::create(&tmp).map_err(StoreError::io("create manifest temp"))?;
    f.write_all(&manifest.encode())
        .map_err(StoreError::io("write manifest temp"))?;
    f.sync_all().map_err(StoreError::io("sync manifest temp"))?;
    drop(f);
    fs::rename(&tmp, manifest_path(root)).map_err(StoreError::io("replace manifest"))
}

/// Reads the manifest and maps its index read-only. The writer may grow the index between the
/// two steps and delete the generation just read, so a missing index file means "read the
/// manifest again" (bounded).
fn map_latest(root: &Path) -> Result<(Manifest, MappedIndex), StoreError> {
    let mut attempts = 0;
    loop {
        let manifest = read_manifest(root)?;
        let path = index_path(root, manifest.generation);
        match MappedIndex::open(&path, manifest.log2_buckets, false) {
            Ok(index) => return Ok((manifest, index)),
            Err(StoreError::Io { source, .. })
                if source.kind() == ErrorKind::NotFound && attempts < 3 =>
            {
                attempts += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn open_append(path: &Path) -> Result<Writer, StoreError> {
    let active = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(StoreError::io("open active segment"))?;
    let active_len = active
        .metadata()
        .map_err(StoreError::io("stat segment"))?
        .len();
    Ok(Writer { active, active_len })
}

impl CustomStore {
    /// Opens the store in `root` for writing, creating it if it does not exist. The caller
    /// guarantees that no other writer uses the same directory.
    pub fn open_writer(root: &Path) -> Result<Self, StoreError> {
        if !manifest_path(root).exists() {
            Self::create(root)?;
        }
        let manifest = read_manifest(root)?;
        let index = MappedIndex::open(
            &index_path(root, manifest.generation),
            manifest.log2_buckets,
            true,
        )?;
        let writer = open_append(&segment_path(root, manifest.segment_count))?;
        Ok(Self::assemble(root, manifest, index, Some(writer)))
    }

    /// Opens an existing store read-only. Constant time: reads the manifest and maps the
    /// index; no segment is read.
    pub fn open_read_only(root: &Path) -> Result<Self, StoreError> {
        let (manifest, index) = map_latest(root)?;
        Ok(Self::assemble(root, manifest, index, None))
    }

    fn create(root: &Path) -> Result<(), StoreError> {
        fs::create_dir_all(root.join("seg")).map_err(StoreError::io("create store directory"))?;
        let manifest = Manifest {
            generation: 1,
            log2_buckets: INITIAL_LOG2_BUCKETS,
            segment_count: 1,
            entry_count: 0,
        };
        remove_if_present(&index_path(root, 1))?;
        MappedIndex::create(&index_path(root, 1), INITIAL_LOG2_BUCKETS)?;
        File::create(segment_path(root, 1)).map_err(StoreError::io("create segment"))?;
        write_manifest(root, manifest)
    }

    fn assemble(
        root: &Path,
        manifest: Manifest,
        index: MappedIndex,
        writer: Option<Writer>,
    ) -> Self {
        Self {
            root: root.to_path_buf(),
            manifest,
            index,
            segments: Vec::new(),
            writer,
            corrupt_misses: 0,
        }
    }

    /// Number of lookups that found an index entry whose record failed validation.
    pub fn corrupt_misses(&self) -> u64 {
        self.corrupt_misses
    }

    /// Index generation currently mapped.
    pub fn generation(&self) -> u32 {
        self.manifest.generation
    }

    /// Re-reads the manifest and remaps the index if another process grew it.
    pub fn refresh(&mut self) -> Result<(), StoreError> {
        let latest = read_manifest(&self.root)?;
        if latest.generation == self.manifest.generation {
            self.manifest = latest;
        } else {
            (self.manifest, self.index) = map_latest(&self.root)?;
        }
        Ok(())
    }

    /// Read handle for `segment`, or `None` if its file does not exist. The caller checked
    /// that `segment` is within `1 ..= segment_count`.
    fn segment(&mut self, segment: u32) -> Result<Option<&File>, StoreError> {
        let slot =
            usize::try_from(segment).map_err(|_| StoreError::BadMetadata("segment number"))?;
        if self.segments.len() < slot {
            self.segments.resize_with(slot, || None);
        }
        let entry = &mut self.segments[slot - 1];
        if entry.is_none() {
            match File::open(segment_path(&self.root, segment)) {
                Ok(file) => *entry = Some(file),
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
                Err(e) => {
                    return Err(StoreError::Io {
                        context: "open segment",
                        source: e,
                    });
                }
            }
        }
        Ok(entry.as_ref())
    }

    /// True if `segment` can exist. A damaged index entry that still passed its 16-bit check
    /// may name any number; a read-only handle first catches up with the writer.
    fn segment_in_range(&mut self, segment: u32) -> Result<bool, StoreError> {
        if segment > self.manifest.segment_count && self.writer.is_none() {
            self.refresh()?;
        }
        Ok(segment != 0 && segment <= self.manifest.segment_count)
    }

    /// Reads and validates the record at `loc`. `Ok(None)` means the record is torn or corrupt.
    fn read_record(&mut self, key: u128, loc: Location) -> Result<Option<Vec<u8>>, StoreError> {
        if !self.segment_in_range(loc.segment)? {
            return Ok(None);
        }
        let Some(file) = self.segment(loc.segment)? else {
            return Ok(None);
        };
        let offset = u64::from(loc.offset_units) * ALIGN;
        let mut header = [0u8; HEADER_LEN];
        if !read_or_eof(file, &mut header, offset)? {
            return Ok(None);
        }
        let Some(h) = record::decode(&header) else {
            return Ok(None);
        };
        if h.key != key || h.stored_len != loc.len || h.kind != u16::from(loc.kind) {
            return Ok(None);
        }
        let len = usize::try_from(h.stored_len).map_err(|_| StoreError::TooLarge(usize::MAX))?;
        let mut payload = vec![0u8; len];
        if !read_or_eof(file, &mut payload, offset + HEADER_LEN as u64)? {
            return Ok(None);
        }
        if record::payload_check(h.kind, key, &payload) != h.payload_check {
            return Ok(None);
        }
        Ok(Some(payload))
    }

    /// Doubles the index into a new generation (synchronous in this prototype).
    fn grow(&mut self) -> Result<(), StoreError> {
        let next = Manifest {
            generation: self.manifest.generation + 1,
            log2_buckets: self.index.log2_buckets() + 1,
            ..self.manifest
        };
        let path = index_path(&self.root, next.generation);
        remove_if_present(&path)?;
        let bigger = MappedIndex::create(&path, next.log2_buckets)?;
        let mut copied = 0;
        for (key, loc) in self.index.entries() {
            bigger.insert(key, loc)?;
            copied += 1;
        }
        // The persisted count lags after a crash or a drop without flush; growth recounts it.
        let next = Manifest {
            entry_count: copied,
            ..next
        };
        bigger.flush()?;
        write_manifest(&self.root, next)?;
        let old = index_path(&self.root, self.manifest.generation);
        self.index = bigger;
        self.manifest = next;
        match fs::remove_file(&old) {
            // A reader in another process may still map the old file, and Windows refuses to
            // delete a mapped file. The stale file is unused and left behind (report item 7).
            Err(e) if e.kind() == ErrorKind::PermissionDenied => Ok(()),
            Err(e) => Err(StoreError::Io {
                context: "remove old index",
                source: e,
            }),
            Ok(()) => Ok(()),
        }
    }

    /// Seals the active segment and starts the next one.
    fn seal(&mut self) -> Result<(), StoreError> {
        let next = self.manifest.segment_count + 1;
        let writer = self
            .writer
            .as_mut()
            .ok_or(StoreError::BadMetadata("read-only store"))?;
        writer
            .active
            .sync_data()
            .map_err(StoreError::io("sync sealed segment"))?;
        *writer = open_append(&segment_path(&self.root, next))?;
        self.manifest.segment_count = next;
        write_manifest(&self.root, self.manifest)
    }

    /// Appends the record and returns where it starts.
    fn append(
        &mut self,
        key: u128,
        kind: u8,
        payload: &[u8],
        len: u32,
    ) -> Result<Location, StoreError> {
        let record_len = HEADER_LEN as u64 + u64::from(len);
        let writer = self
            .writer
            .as_ref()
            .ok_or(StoreError::BadMetadata("read-only store"))?;
        if writer.active_len > 0
            && record::align_up(writer.active_len) + record_len > SEGMENT_SEAL_BYTES
        {
            self.seal()?;
        }
        let writer = self
            .writer
            .as_mut()
            .ok_or(StoreError::BadMetadata("read-only store"))?;
        let start = record::align_up(writer.active_len);
        let pad = usize::try_from(start - writer.active_len).unwrap_or(0);
        let tail = usize::try_from(record::align_up(record_len) - record_len).unwrap_or(0);
        let mut prefix = vec![0u8; pad];
        prefix.extend_from_slice(&record::encode(u16::from(kind), key, payload, len));
        let io = StoreError::io("append record");
        writer.active.write_all(&prefix).map_err(io)?;
        writer
            .active
            .write_all(payload)
            .map_err(StoreError::io("append payload"))?;
        writer
            .active
            .write_all(&[0u8; 16][..tail])
            .map_err(StoreError::io("append padding"))?;
        writer.active_len = start + record::align_up(record_len);
        let offset_units =
            u32::try_from(start / ALIGN).map_err(|_| StoreError::TooLarge(payload.len()))?;
        Ok(Location {
            segment: self.manifest.segment_count,
            offset_units,
            len,
            kind,
        })
    }
}

/// Positioned read that reports end-of-file (a truncated segment) as `Ok(false)`.
fn read_or_eof(file: &File, buf: &mut [u8], offset: u64) -> Result<bool, StoreError> {
    match platform::read_exact_at(file, buf, offset) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(StoreError::Io {
            context: "read record",
            source: e,
        }),
    }
}

fn remove_if_present(path: &Path) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(StoreError::Io {
            context: "remove stale file",
            source: e,
        }),
        Ok(()) | Err(_) => Ok(()),
    }
}

impl BlobStore for CustomStore {
    fn put(&mut self, key: u128, kind: u8, payload: &[u8]) -> Result<(), StoreError> {
        if self.index.lookup(key).is_some() {
            return Ok(());
        }
        let len = u32::try_from(payload.len()).map_err(|_| StoreError::TooLarge(payload.len()))?;
        if (self.manifest.entry_count + 1) * 10 > self.index.buckets() * 7 {
            self.grow()?;
        }
        let loc = self.append(key, kind, payload, len)?;
        let mut outcome = self.index.insert(key, loc)?;
        if outcome == Insert::Full {
            // Only possible when the persisted count lagged; growing recounts it.
            self.grow()?;
            outcome = self.index.insert(key, loc)?;
        }
        match outcome {
            Insert::Inserted => self.manifest.entry_count += 1,
            Insert::AlreadyPresent => {}
            Insert::Full => return Err(StoreError::BadMetadata("index full after growth")),
        }
        Ok(())
    }

    fn get(&mut self, key: u128) -> Result<Option<Vec<u8>>, StoreError> {
        let mut loc = self.index.lookup(key);
        if loc.is_none() && self.writer.is_none() {
            self.refresh()?;
            loc = self.index.lookup(key);
        }
        let Some(loc) = loc else { return Ok(None) };
        let found = self.read_record(key, loc)?;
        if found.is_none() {
            self.corrupt_misses += 1;
        }
        Ok(found)
    }

    fn flush(&mut self) -> Result<(), StoreError> {
        let writer = self
            .writer
            .as_ref()
            .ok_or(StoreError::BadMetadata("read-only store"))?;
        writer
            .active
            .sync_data()
            .map_err(StoreError::io("sync segment"))?;
        self.index.flush()?;
        write_manifest(&self.root, self.manifest)
    }

    fn reader(&self) -> Result<Box<dyn BlobStore>, StoreError> {
        Ok(Box::new(Self::open_read_only(&self.root)?))
    }
}

#[cfg(test)]
mod tests {
    use super::{CustomStore, Manifest};
    use crate::index::Location;
    use crate::store::BlobStore;

    #[test]
    fn manifest_round_trips_and_rejects_corruption() {
        let m = Manifest {
            generation: 3,
            log2_buckets: 12,
            segment_count: 7,
            entry_count: 9000,
        };
        let bytes = m.encode();
        assert_eq!(Manifest::decode(&bytes).unwrap(), m);
        for byte in 0..bytes.len() {
            let mut bad = bytes;
            bad[byte] ^= 0x10;
            assert!(Manifest::decode(&bad).is_err(), "byte {byte} flip accepted");
        }
    }

    #[test]
    fn entry_pointing_at_a_missing_segment_is_a_miss() {
        let dir = std::env::temp_dir().join(format!("lince-poc6-badseg-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        let mut store = CustomStore::open_writer(&dir).unwrap();
        store.put(1, 1, b"real").unwrap();
        // Simulates multi-bit index damage that still passes the 16-bit entry check.
        for (key, segment) in [(2u128, 7u32), (3, 1_000_000), (4, 0)] {
            let loc = Location {
                segment,
                offset_units: 0,
                len: 4,
                kind: 1,
            };
            store.index.insert(key, loc).unwrap();
            assert_eq!(store.get(key).unwrap(), None, "segment {segment}");
        }
        let mut reader = CustomStore::open_read_only(&dir).unwrap();
        assert_eq!(reader.get(2).unwrap(), None);
        assert_eq!(reader.get(1).unwrap().as_deref(), Some(&b"real"[..]));
        drop((store, reader));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
