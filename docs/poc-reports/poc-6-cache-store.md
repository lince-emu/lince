# PoC-6 — Persistent cache indexing and lazy access

- **Status:** Prototype and harness ready; **no measurements yet**. Measurements on the
  maintainer's hardware are a follow-up (T-010 acceptance).
- **Decides:** ADR-0015 (cache persistence format), ADR-0016 (indexing and lazy loading).
- **Code:** [`poc/cache-store/`](../../poc/cache-store/README.md).

## Hardware and drivers

| Item | Value |
|------|-------|
| CPU | *(to fill)* |
| RAM | *(to fill)* |
| Storage (NVMe) | *(model, file system)* |
| Storage (HDD) | *(model, file system)* |
| OS | *(version)* |
| Rust toolchain | *(rustc version)* |

## Method

- **Workload:** seeded, log-uniform blob sizes from 1 KiB to 2 MiB (`--dist` to change). The
  roadmap's "Lichii-like" distribution has no measured source yet, so log-uniform is a
  placeholder to replace once real cache contents are profiled.
- **Open time:** `harness open-time`, 20 runs per engine and size (1 MiB, 64 MiB, 1 GiB,
  4 GiB). "Open" covers what an engine does before its first lookup: custom = read
  `MANIFEST`, map the index, open the active segment; redb = `Database::create` on the existing
  file; SQLite = open, set WAL and `synchronous`, read the schema.
- **Resolve latency:** `harness resolve`, 10,000 random lookups of existing keys. Each sample
  covers index lookup, positioned read and checksum validation. Run with `--writer off` and
  `--writer on`; with `on`, another thread appends new artifacts throughout.
- **Engine settings:** redb commits each `put` with `Durability::None` and makes it durable
  in `flush` with a `Durability::Immediate` commit. SQLite uses WAL with
  `synchronous = NORMAL`, and `flush` runs a passive checkpoint. The custom store appends
  without fsync, and `flush` syncs the active segment, the index and the manifest.
- **Concurrency:** for `--writer on`, the background writer is a thread of the harness
  process. The custom store's reader maps the same files independently. SQLite opens a
  second connection. redb's reader shares the writer's `Database`, because redb does not
  allow a second process to open the file. A reader in another process is required by doc 02
  §5 step 1 (a second instance becomes read-only) and by the PoC-6 correctness list
  ("concurrent reader process"), so this matters for the verdict. The custom store's separate-process reading is exercised by the
  fault tests, but its latency is not timed.
- **Repeated writer-on runs:** each run appends keys no earlier run used, and the CSV
  records the items actually on disk.
- **Cache state:** warm OS cache by default. Cold-cache runs need the OS file cache dropped
  between runs (administrator rights, OS-specific). Record which was used.
- **Correctness:** `cargo test --features sqlite`. The fault-injection suite truncates
  segments, flips bits in segments and the index, runs a reader in this process while another
  process writes, and kills that writer mid-append.

## Prototype simplifications

These differ from doc 02 and must be considered when reading results:

1. The payload checksum is CRC-32C (table-driven, eight bytes per step), not XXH3-64. No hash
   dependency was approved for the prototype. Its cost relative to XXH3-64 on 2 MiB blobs is
   unmeasured and is a factor to measure before comparing resolve latencies.
2. There is no `store.lock`. The single-writer rule is enforced by the caller.
3. Index growth runs synchronously on the writer, not in the background.
4. There is no background tail scan. Records written but not indexed before a crash become
   misses.
5. Corrupt entries are not tombstoned. A key whose record is damaged (bit flip, truncation,
   or power loss that persisted the index page but not the segment data) therefore stays a
   miss until the store is rebuilt, because `put` sees the existing entry and does not
   re-append. Codec is always raw (no lz4).
6. Same-process readers do not use an in-memory overlay. They map the same index file, see
   inserts immediately through atomics, and remap after a miss when the manifest generation
   changed.
7. On Windows, an old index file still mapped by a reader cannot be deleted after growth, so
   it is left behind.
8. `MANIFEST` replacement is write-temp → fsync → rename, without an fsync of the parent
   directory afterwards.
9. The persisted entry count lags after a crash or a drop without flush; index growth recounts
   it, and a full table triggers growth instead of an error.

## Results

*(Empty until measured. Attach or summarize `results/open_time.csv` and
`results/resolve.csv`; never fill in expected numbers.)*

| Engine | 1 MiB P99 open | 64 MiB | 1 GiB | 4 GiB | Max − min |
|--------|----------------|--------|-------|-------|-----------|
| custom | | | | | |
| redb | | | | | |
| SQLite | | | | | |

| Engine | Writer | Resolve P50 | P99 | P99.9 | Max |
|--------|--------|-------------|-----|-------|-----|
| custom | off / on | | | | |
| redb | off / on | | | | |
| SQLite | off / on | | | | |

Still to measure (roadmap benchmark list): recovery time after a kill, space amplification
after churn, and runs on HDD.

## Comparison with the hypothesis

*(After measurement: custom open ≤ 10 ms P99 for 1 MiB–4 GiB, ≤ 2 ms spread between sizes.)*

## Verdict

*(Success or failure per docs/06-poc-roadmap.md PoC-6. Under ADR-0015's revisit condition,
`redb` or SQLite within 20% on every metric means adopting it.)*

## ADR changes proposed

*(After measurement.)*
