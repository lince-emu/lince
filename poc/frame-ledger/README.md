# PoC-7 — Frame ledger

Standalone prototype for [PoC-7](../../docs/06-poc-roadmap.md) (enables ADR-0027). Report:
[`docs/poc-reports/poc-7-frame-ledger.md`](../../docs/poc-reports/poc-7-frame-ledger.md).

- **Objective:** always-on event logging under 1% frame overhead.
- **Success criteria:** event cost ≤ 30 ns, and the analyzer finds injected synthetic spikes
  with the correct cause.
- **Not included:** the Tracy bridge, which needs a dependency.

## Layout

| Path | Content |
|------|---------|
| `src/event.rs` | Event kinds from doc 05 §3, thread roles, 24-byte encoding |
| `src/ring.rs` | Single-producer ring with one seqlock per slot; never blocks, overwrites the oldest event |
| `src/ledger.rs` | `Ledger` (lock-free registration and dumps) and the per-thread `Recorder` |
| `src/dump.rs` | Versioned binary dump format |
| `src/analyzer.rs` | Frame-time percentiles and spike attribution (doc 05 §4) |
| `src/bin/bench.rs` | Event-cost microbenchmark, writes `results/event_cost.csv` (gitignored) |
| `tests/` | Concurrent dumps without torn events, dump format, and analyzer acceptance with injected spikes |

The crate is std-only, has no dependencies and uses no `unsafe`. A `Recorder` can move to
another thread but cannot be shared, which keeps every ring single-producer.

## Running

```bash
cargo test
cargo build --release
./target/release/bench --events 10000000 --runs 5
```

The benchmark has three scenarios:

- `record`: reads the clock for every event.
- `record_at`: uses a fixed timestamp, so it measures the ring cost alone.
- `record_with_dumper`: like `record`, while another thread dumps the ledger in a tight loop,
  which is the worst case for an always-on reader.
