# ADR-0015: Cache persistence format

- **Status:** Proposed (needs PoC-6)
- **Date:** 2026-11-13
- **Related:** doc 02 §4, §7, §9; ADR-0014, ADR-0016, ADR-0026

## Context
The workload is write-once immutable blobs (1 KiB–2 MiB), point lookups by 128-bit key, no
range queries, no updates, rare deletes (eviction), occasional compaction, one writer process.

## Requirements
Constant-time open; point lookup without parsing; crash safety without fsync on critical
threads; corruption detection per record; pure Rust preferred.

## Alternatives
1. **Custom append-only segment store + memory-mapped hash index** (doc 02 §4).
2. `redb` (pure Rust embedded ACID KV).
3. SQLite in WAL mode (C dependency).
4. LMDB via `heed` (C dependency, mmap B+tree).
5. One file per artifact (filesystem as the database).

## Decision
Provisionally option 1, benchmarked against 2 and 3 in PoC-6. Option 5 rejected (millions of
small files: slow on Windows, antivirus scanning, inode pressure). Option 4 rejected unless
PoC-6 shows the others fail (C dependency, ADR-0026).

## Technical justification
The workload fits a log-structured content-addressed store exactly; general databases bring
transactional fsync patterns and page caches we do not need. A comparator keeps us honest.

## Advantages
Minimal I/O; trivial crash model (append + checksums); full control of I/O priority.

## Disadvantages
Custom code (~1–2k lines) needing careful testing.

## Risks
Subtle durability bugs. Mitigation: fault-injection suite; corruption = miss.

## Validation plan
PoC-6: open time vs. size (1 MiB → 4 GiB), lookup P99.9 under concurrent appends, recovery
time after kill -9 at random points, space amplification after churn.

## Conditions for revisiting
`redb` or SQLite within 20% on every PoC-6 metric **and** able to serve a read-only second
process (doc 02 §5) → adopt it and delete custom code (decision D-002).
