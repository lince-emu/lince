# PoC-7 — Frame ledger

- **Status:** The prototype, the analyzer and the event-cost benchmark are done. Event cost was
  measured on one machine (below). A loom model of the ring passes (T-021); the run on ARM
  hardware is pending. The Tracy bridge is not part of this PoC.
- **Enables:** ADR-0027 (observability).
- **Code:** [`poc/frame-ledger/`](../../poc/frame-ledger/README.md).
- **Scope:** The maintainer approved this as a single PR (~850 non-test lines, above the
  400-line guideline).

## Hardware and software

| Item | Value |
|------|-------|
| CPU | AMD Ryzen 5 7600 6-Core Processor |
| RAM | 32 GB |
| OS | Microsoft Windows 11 Pro |
| Rust toolchain | rustc 1.99.0 (b940084d7 2026-09-28), `--release` |
| Date | 2026-11-13 |

## Method

- **Events:** An event is 24 bytes: timestamp, frame number, kind and argument. Each ring slot
  adds an 8-byte sequence word.
- **Duration events:** An event whose argument is a duration has `ts` = the end of the
  interval.
- **Ring:** 2^16 slots per thread, one producer per ring.
  - Every slot is a seqlock in the fence-based form described by Boehm (MSPC 2012).
  - Recording never blocks and never allocates. When the ring is full, the oldest events are
    overwritten.
  - A dump keeps, per thread, only the newest contiguous run of events. It also records
    how many older events are missing, including any that were overwritten during the copy.
- **Benchmark:** `bench --events 10000000 --runs 5` measures one recording thread. Cost per
  event = total time ÷ 10,000,000. The event kind is passed through `black_box` so that its
  encoding is not constant-folded. Three scenarios:
  - `record`: reads `std::time::Instant` for every event.
  - `record_at`: uses a fixed timestamp, so it measures the ring alone.
  - `record_with_dumper`: `record` while a second thread calls `Ledger::dump` in a tight
    loop. This is the worst case; a real dump runs on demand or after a spike.
- **Analyzer:**
  - Blocking time on render-critical threads (guest cores, GPU front, GPU back, submit,
    present) is clipped to the frame, and overlapping intervals are merged, so concurrent
    waits count once.
  - A spike is emulator-induced when the merged time covers ≥ 50 % of the overrun (doc 05 §4).
  - Spikes earlier than the point where a critical ring lost events are reported as "evidence
    incomplete", with no verdict.
  - `ThreadPreempted` is not counted as emulator time, because doc 05 §4 treats preemption as
    a separate class.
- **Analyzer acceptance:** `tests/analyzer.rs` records a synthetic session of 1,000 frames of
  ~16.7 ms with ±0.5 ms deterministic jitter. It injects five spikes against a 20 ms target
  and checks four things: the reported cause, the verdict, P99, and P99.9. Further tests cover:
  - concurrent waits on three critical threads;
  - a wait that started before its frame;
  - a frame whose evidence was overwritten.
- **Memory-model check (T-021, D-004):** `ring::loom_tests` in `src/ring.rs` is a
  [loom](https://crates.io/crates/loom) 0.7.3 model. `loom` is a dev-dependency that is
  resolved only under `--cfg loom`; in that build the ring takes `AtomicU64` and `fence` from
  loom instead of std, and nothing else in the ring changes.
  - **Model:** a ring with one slot. Event 0 is pushed first. Then a producer thread pushes
    events 1 and 2 while the model thread takes a snapshot, so every push overwrites the event
    the snapshot may be copying. Each word of each event has its own value.
  - **Property:** entry `i` of a snapshot that reports `head` events must equal event
    `head - len + i`, all three words. A torn copy, or a stale event accepted under a newer
    number, fails the assertion.
  - **Command:** `RUSTFLAGS="--cfg loom" cargo test --release --lib loom` in
    `poc/frame-ledger/`, rustc 1.100.0, on the machine above.
  - **Fence check:** the same command with each fence deleted from a copy of `src/ring.rs`,
    one at a time: the release fence in `Ring::push` and the acquire fence in
    `Ring::snapshot`. The removals were not committed.

## Results

### Event cost (ns per event, 5 runs)

| Scenario | Run 0 | Run 1 | Run 2 | Run 3 | Run 4 |
|----------|-------|-------|-------|-------|-------|
| `record` | 27.369 | 27.367 | 27.679 | 28.261 | 27.784 |
| `record_at` (ring only) | 2.612 | 2.483 | 2.587 | 2.569 | 2.417 |
| `record_with_dumper` | 31.512 | 31.326 | 30.835 | 31.239 | 30.940 |

### Analyzer

`cargo test` passes (15 tests). All five injected spikes are found, and only those five:

| Frame | Injected | Reported cause | Emulator-induced |
|-------|----------|----------------|------------------|
| 100 | 40 ms frame, 22 ms `PipelineWait` on GPU back | `PipelineWait` | yes |
| 400 | 30 ms frame, 12 ms `CacheResolve` on GPU front | `CacheResolve` | yes |
| 600 | 35 ms frame, no emulator events | none | no |
| 800 | 35 ms frame, 50 ms `CompileEnd` on a worker (not render-critical) | none | no |
| 900 | 30 ms frame, 2 ms `FenceWait` + 1 ms `PipelineWait`, not overlapping | `FenceWait` | no (3 ms < half of 10 ms) |

The other analyzer tests:

- Three concurrent 2 ms waits on three critical threads are counted as 2 ms, not 6 ms.
- A wait that started before its frame counts only for its part inside the frame.
- A spike whose GPU-back events were overwritten is reported as evidence incomplete, not as
  the game's own behavior.

### Loom model

| Ring as built | Result |
|---------------|--------|
| Unchanged | Passes. Loom completed 89,441 executions (`LOOM_LOG=info`) in about 5 s. |
| Without `fence(Ordering::Release)` in `Ring::push` | Fails: `torn or stale entry for event 0`, `left: [1, 2, 6]`, `right: [1, 2, 3]` |
| Without `fence(Ordering::Acquire)` in `Ring::snapshot` | Fails with the same assertion and values |

In both failing runs the snapshot accepted event 0 with its third word taken from event 1
(`6`): the reader saw the producer's new data but still read the old sequence word. So loom
detects the removal of either fence, and the model needs both.

### ARM run

**Pending.** No ARM host was available; this machine is x86-64, whose strong memory ordering
cannot expose a missing fence. On any ARM64 host (Linux, macOS or Windows on ARM), from
`poc/frame-ledger/`:

```bash
rustc -vV
cargo test --release
for i in $(seq 1 100); do
  cargo test --release --test ledger dumps_taken_during_recording_never_contain_torn_events \
    || break
done
RUSTFLAGS="--cfg loom" cargo test --release --lib loom
```

Record here: the host (CPU or board model), the OS and its version, the `rustc -vV` output
(host triple and version), how many of the 100 stress repetitions passed, and the result of
the other two commands.

## Comparison with the hypothesis

- **Event cost ≤ 30 ns:**
  - Met for `record` on this machine: 27.4–28.3 ns.
  - Not met with a concurrent dumper in a tight loop: 30.8–31.5 ns.
- **Where the cost goes:** The ring itself costs 2.4–2.6 ns, so most of the `record` cost is
  the `Instant::now()` call. This is an inference from the difference between `record` and
  `record_at`; the clock was not timed on its own.
- **Analyzer:** It attributes the injected duration-based causes. It ignores compile time on
  non-critical threads. It reports zero or below-threshold emulator coverage for spikes
  without emulator events. Doc 05 §4 also requires a reproducibility check before calling a
  spike "the game's own behavior"; that is not implemented.
- **Not covered by the analyzer:** these classes from doc 05 §4:
  - allocations, uploads and evictions (their arguments are sizes, not durations);
  - shader discovery and compile starts within a frame;
  - thread busy ≥ 95 %;
  - present delay;
  - guest-core idle time;
  - OS and thermal causes;
  - events recorded after a ring was snapshotted (`Ledger::dump` copies rings one after
    another).
- **Not measured or not verified:**
  - other CPUs and OSes;
  - many recording threads at once;
  - ring sizing, which ADR-0027's validation plan lists and which is still open;
  - overhead inside a real frame loop, i.e. the < 1 % criterion.
  - The memory-ordering protocol is checked by the loom model above and by a stress test on
    x86-64. Loom explores only the weak-memory behaviors it models, and the model covers one
    producer, one reader and one slot. There is no run on weak-memory hardware yet (see
    "ARM run").

## Verdict

**Partial success.**

- The analyzer meets its criterion for duration-based causes.
- Event cost meets ≤ 30 ns without a concurrent reader, on one machine. With a reader dumping
  continuously, it misses by about 1–1.5 ns.
- One CPU is not enough to accept ADR-0027's overhead budget. The loom model checks the ring's
  memory ordering, but the run on ARM hardware is still pending.

## ADR and design changes proposed

None applied. To decide with ADR-0027:

1. **Doc 05 §3:** events that carry a duration (`CompileEnd`, `PipelineWait`, `CacheResolve`,
   `CacheWrite`, `CacheFsync`, `FenceWait`) should put the duration in their argument, with
   `ts` = end of interval. Doc 05 currently gives `CompileStart/End` a tier argument only.
   The analyzer needs durations to attribute time.
2. **Before acceptance:**
   - Run the test suite on ARM hardware (see "ARM run"). The loom model is done (T-021).
   - Re-run on the other tier-1 platforms and with several recording threads.
   - Measure ring sizing and frame-loop overhead.
3. **Timestamp source:** Measure a cheaper source, such as the CPU timestamp counter. That
   needs platform code and `unsafe` in an `[unsafe]` crate.
