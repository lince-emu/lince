# PoC-6 — Persistent cache store

Standalone prototype for [PoC-6](../../docs/06-poc-roadmap.md) (decides ADR-0015 and
ADR-0016). Report: [`docs/poc-reports/poc-6-cache-store.md`](../../docs/poc-reports/poc-6-cache-store.md).

- **Objective:** validate O(1) open and fast resolves; pick the storage engine.
- **Hypothesis:** the custom store opens in ≤ 10 ms P99 for 1 MiB–4 GiB, with ≤ 2 ms
  difference between sizes.
- **Alternatives compared:** custom store (segments + memory-mapped index,
  [doc 02 §4](../../docs/02-cache-architecture.md)), `redb`, SQLite in WAL mode.

## Layout

| Path | Content |
|------|---------|
| `src/custom.rs`, `src/index.rs`, `src/record.rs` | The custom store: manifest, segments, mapped index, record framing |
| `src/redb_store.rs`, `src/sqlite_store.rs` | Comparators behind the same `BlobStore` trait |
| `src/workload.rs` | Seeded workload; payloads are a function of the key, so every read is verifiable |
| `src/bin/harness.rs` | Measurement harness writing CSV to `results/` (gitignored) |
| `tests/` | Engine behavior, index growth, segment sealing, fault injection |

## Running

```bash
cargo test --features sqlite
cargo build --release --features sqlite
./target/release/harness open-time --engine custom --dir /path/on/test/disk --sizes 1M,64M,1G,4G --runs 20
./target/release/harness resolve --engine custom --dir /path/on/test/disk --size 1G --lookups 10000 --writer on
```

Repeat with `--engine redb` and `--engine sqlite`. The first run for a size populates
`<dir>/<engine>-<bytes>/` and later runs reuse it. `--dist loguniform:MIN:MAX` or
`uniform:MIN:MAX` sets the blob-size distribution (default: log-uniform 1 KiB–2 MiB).

Output:

- `results/open_time.csv`: `engine,target_bytes,stored_bytes,items,run,open_ns`
- `results/resolve.csv`: `engine,target_bytes,items_on_disk,writer,background_puts,lookups,p50_ns,p99_ns,p999_ns,max_ns,misses`

If a `--writer on` run fails part-way, the items it already wrote stay on disk but are not
counted in `workload.meta`; use a fresh `--dir` after a failed run.

The harness measures with a warm OS cache. For cold-cache numbers, drop the OS file cache
between runs. That needs administrator rights and is OS-specific, so the harness does not
do it.

## Fault-injection tests

`tests/faults.rs` truncates segments at random points, flips random bits in segments and in the
index, runs a reader in this process while `harness append-forever` writes from another
process, then kills that writer mid-append and reopens the store as a writer. Every lookup
must return the exact payload or a miss.
