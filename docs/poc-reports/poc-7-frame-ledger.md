# PoC-7 — Frame ledger

- **Status:** The prototype, the analyzer and the event-cost benchmark are done. Event cost was
  measured on one machine (below). The Tracy bridge is not part of this PoC.
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
  - The memory-ordering protocol was checked by argument and by a stress test on x86-64 only.
    x86-64 orders memory strongly, so the test cannot catch a missing or weakened fence.
    There is no loom model and no run on weak-memory hardware.

## Verdict

**Partial success.**

- The analyzer meets its criterion for duration-based causes.
- Event cost meets ≤ 30 ns without a concurrent reader, on one machine. With a reader dumping
  continuously, it misses by about 1–1.5 ns.
- One CPU and an x86-only concurrency test are not enough to accept ADR-0027's overhead
  budget.

## ADR and design changes proposed

None applied. To decide with ADR-0027:

1. **Doc 05 §3:** events that carry a duration (`CompileEnd`, `PipelineWait`, `CacheResolve`,
   `CacheWrite`, `CacheFsync`, `FenceWait`) should put the duration in their argument, with
   `ts` = end of interval. Doc 05 currently gives `CompileStart/End` a tier argument only.
   The analyzer needs durations to attribute time.
2. **Before acceptance:**
   - Add a loom model (or an equivalent weak-memory check) of the ring.
   - Re-run on the other tier-1 platforms and with several recording threads.
   - Measure ring sizing and frame-loop overhead.
3. **Timestamp source:** Measure a cheaper source, such as the CPU timestamp counter. That
   needs platform code and `unsafe` in an `[unsafe]` crate.
