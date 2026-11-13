# PoC-6 — Persistent cache indexing and lazy access

- **Status:** Prototype and harness ready. Only the checksum cost is measured (T-020); open
  time and resolve latency on the maintainer's hardware are still a follow-up (T-010
  acceptance).
- **Decides:** ADR-0015 (cache persistence format), ADR-0016 (indexing and lazy loading).
- **Code:** [`poc/cache-store/`](../../poc/cache-store/README.md).

## Hardware and drivers

| Item | Value |
|------|-------|
| CPU | AMD Ryzen 5 7600 (6 cores, 12 threads), for the checksum cost |
| RAM | 32 GiB, for the checksum cost |
| Storage (NVMe) | *(model, file system)* |
| Storage (HDD) | *(model, file system)* |
| OS | Microsoft Windows 11 Pro 10.0.26300, for the checksum cost |
| Rust toolchain | rustc 1.100.0, release profile, for the checksum cost |

## Method

- **Workload:** seeded, log-uniform blob sizes from 1 KiB to 2 MiB (`--dist` to change). The
  roadmap's "Lichii-like" distribution has no measured source yet, so log-uniform is a
  placeholder to replace once real cache contents are profiled.
- **Open time:** `harness open-time`, 20 runs per engine and size (1 MiB, 64 MiB, 1 GiB,
  4 GiB). "Open" covers what an engine does before its first lookup: custom = read
  `MANIFEST`, map the index, open the active segment; redb = `Database::create` on the existing
  file in `SingleWriter` mode; SQLite = open, set WAL and `synchronous`, read the schema.
- **Resolve latency:** `harness resolve`, 10,000 random lookups of existing keys. Each sample
  covers index lookup, positioned read and checksum validation. Run with `--writer off` and
  `--writer on`; with `on`, a thread of the harness process appends new artifacts throughout.
  The timed reader runs in a child process (`--reader process`, the default), as a second
  emulator instance would (doc 02 §5 step 1 and §10, decision D-002); `--reader thread` keeps
  it in the harness process for comparison.
- **Engine settings:** redb runs in `ConcurrencyMode::SingleWriter` (writer and readers).
  That mode refuses non-durable commits, so the redb writer buffers each `put` and `flush`
  inserts the batch in one `Durability::Immediate` transaction; redb readers therefore see new
  artifacts only after a flush. SQLite uses WAL with `synchronous = NORMAL`, and `flush` runs
  a passive checkpoint. The custom store appends
  without fsync, and `flush` syncs the active segment, the index and the manifest.
- **Second-process reader (T-024):** each `resolve.csv` row records the reader setup
  (`reader` = `process` or `thread`), how the reader shares the store (`sharing`) and whether
  the engine served it (`served`):

  | Engine | `sharing` | Second-process reader |
  |--------|-----------|-----------------------|
  | custom | `mapped_read_only`: maps the segments and index read-only | Served |
  | redb | `redb_single_writer`: `Builder::open_read_only` in the writer's `SingleWriter` mode | Served |
  | SQLite | `wal_query_only`: a WAL connection with `query_only = ON` | Served |

  "Served" was observed in smoke runs on the machine above (`--size 4M --lookups 2000
  --writer on`) and is checked by `tests/second_process.rs` for every engine. Smoke-run
  latencies are not results and are not reported.
- **Repeated writer-on runs:** each run appends keys no earlier run used, and the CSV
  records the items actually on disk.
- **Cache state:** warm OS cache by default. Cold-cache runs need the OS file cache dropped
  between runs (administrator rights, OS-specific). Record which was used.
- **Checksum cost:** `harness checksum --samples 200 --seed 1` times CRC-32C (this crate's
  table-driven implementation) and the record's seeded XXH3-64 (`xxhash-rust` 0.8.19,
  decision D-001) on seeded random payloads held in CPU cache, so it measures compute only.
  Each sample checksums the payload back to back until it has covered at least 1 MiB and
  records the mean, because Windows' `Instant` ticks every 100 ns.
- **Correctness:** `cargo test --features sqlite`. The fault-injection suite truncates
  segments, flips bits in segments and the index, runs a reader in this process while another
  process writes, and kills that writer mid-append.

## Prototype simplifications

These differ from doc 02 and must be considered when reading results:

1. There is no `store.lock`. The single-writer rule is enforced by the caller.
2. Index growth runs synchronously on the writer, not in the background.
3. There is no background tail scan. Records written but not indexed before a crash become
   misses.
4. Corrupt entries are not tombstoned. A key whose record is damaged (bit flip, truncation,
   or power loss that persisted the index page but not the segment data) therefore stays a
   miss until the store is rebuilt, because `put` sees the existing entry and does not
   re-append. Codec is always raw (no lz4).
5. Same-process readers do not use an in-memory overlay. They map the same index file, see
   inserts immediately through atomics, and remap after a miss when the manifest generation
   changed.
6. On Windows, an old index file still mapped by a reader cannot be deleted after growth, so
   it is left behind.
7. `MANIFEST` replacement is write-temp → fsync → rename, without an fsync of the parent
   directory afterwards.
8. The persisted entry count lags after a crash or a drop without flush; index growth recounts
   it, and a full table triggers growth instead of an error.

## Results

### Checksum cost (T-020)

Mean time per checksum, P50 and P99 over 200 samples, in microseconds. Run with seed 1.
"XXH3-64" is the record's path: seeded with a kind and key, including deriving the seed.
"CRC-32C" is this crate's table-driven software implementation, not the hardware-accelerated
CRC-32C that doc 04 plans, so the ratio does not apply to that one.

| Payload | CRC-32C (table-driven) P50 / P99 | XXH3-64 (seeded) P50 / P99 | CRC-32C ÷ XXH3-64 at P50 |
|---------|----------------------------------|----------------------------|--------------------------|
| 1 KiB | 0.37 / 0.55 | 0.045 / 0.055 | 8.2× |
| 4 KiB | 1.49 / 2.19 | 0.115 / 0.125 | 12.9× |
| 16 KiB | 5.95 / 8.07 | 0.393 / 0.431 | 15.1× |
| 64 KiB | 23.8 / 30.1 | 1.52 / 2.31 | 15.7× |
| 256 KiB | 95.4 / 126.0 | 5.98 / 6.38 | 16.0× |
| 1 MiB | 382.3 / 484.3 | 24.2 / 28.3 | 15.8× |
| 2 MiB | 767.8 / 1,001.3 | 48.4 / 72.8 | 15.9× |

At P50, CRC-32C ran at about 2.7–2.8 GB/s at every size and XXH3-64 at 23 GB/s (1 KiB) to
44 GB/s. The payloads were held in CPU cache (the largest, 2 MiB, fits in this CPU's L3), so
these are compute costs. A resolve reads its payload from the page cache through the mapping,
and its checksum may run slower when the data comes from DRAM.

The record format now uses XXH3-64 for payloads, as doc 02 §4.1 specifies, so this is no
longer a simplification. Resolve latencies measured from now on include the XXH3-64 cost,
which is about 0.05 ms at P50 for a 2 MiB payload on this CPU. The manifest version is now 2,
so a store populated with the old CRC-32C payload checksums fails to open instead of turning
every lookup into a miss.

### Open time and resolve latency

*(Empty until measured. Attach or summarize `results/open_time.csv` and
`results/resolve.csv`; never fill in expected numbers.)*

| Engine | 1 MiB P99 open | 64 MiB | 1 GiB | 4 GiB | Max − min |
|--------|----------------|--------|-------|-------|-----------|
| custom | | | | | |
| redb | | | | | |
| SQLite | | | | | |

| Engine | Writer | Reader | Served | Resolve P50 | P99 | P99.9 | Max |
|--------|--------|--------|--------|-------------|-----|-------|-----|
| custom | off / on | process | | | | | |
| redb (`SingleWriter`) | off / on | process | | | | | |
| SQLite | off / on | process | | | | | |

Still to measure (roadmap benchmark list): recovery time after a kill, space amplification
after churn, and runs on HDD.

## Comparison with the hypothesis

*(After measurement: custom open ≤ 10 ms P99 for 1 MiB–4 GiB, ≤ 2 ms spread between sizes.)*

## Verdict

*(Success or failure per docs/06-poc-roadmap.md PoC-6. Under ADR-0015's "Conditions for
revisiting", `redb` or SQLite is adopted only if it is within 20% on every PoC-6 metric **and**
able to serve a read-only second process (doc 02 §5, §10).)*

## ADR changes proposed

*(After measurement.)*
