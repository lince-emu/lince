# 05 — Performance Validation and Frame-Time Attribution

Status: Proposed · Related ADRs: 0017, 0018, 0023 · PoCs: PoC-7, PoC-8, PoC-10

## 1. What success means

Lince succeeds when frame delivery is consistent across a whole session, from an empty
cache, and every remaining spike can be attributed to a cause outside the emulator. Average
FPS is reported but never used as a pass/fail criterion.

## 2. Metrics

**Frame delivery:** average FPS; 1% and 0.1% lows; median, P95, P99, P99.9 frame time;
frame-time variance; missed deadlines (frames presented later than the target interval);
longest frame; count of frames > 2× target.

**Attribution:** per-thread CPU utilization; GPU utilization (timestamp-based busy time);
CPU/GPU sync waits; shader translation time; Tier-0 execution cost (GPU time per draw);
pipeline compile time; pipeline-wait time on the critical path (must be ~0); cache open time,
lookup latency, hit/miss by layer, memory usage; worker utilization and queue depth; resource
upload and eviction time; memory consumption.

## 3. The frame ledger

An always-on, lock-free, per-thread ring buffer of fixed-size events (target overhead < 1% of a
frame, verified in PoC-7). Every event carries a monotonic timestamp, thread role, event kind
and a frame sequence number:

`FrameBegin/End`, `PresentQueued/Presented`, `GuestFrameFence`, `ShaderDiscovered`,
`Tier0Used`, `CompileStart/End{tier}`, `PipelineWait`, `CacheResolve`, `CacheWrite`,
`CacheFsync`, `Alloc{size}`, `Upload{bytes}`, `Evict`, `FenceWait`, `QueueSubmit`,
`MacroBurst`, `HleCall{service}`, `GovernorThrottle{level}`, `ThreadPreempted` (sampled).

Events for blocking work — `PipelineWait`, `FenceWait`, `CacheResolve`, `CacheWrite`,
`CacheFsync` and `CompileEnd` — carry their duration in nanoseconds as the argument, and their
timestamp is the end of the interval (decision D-003). `CompileStart` stays a separate event so
compiles in progress are visible in a dump.

`lince-frame-analyzer` turns a ledger dump into the report below.

## 4. Spike classification (charter §13.2)

For every frame over target, the analyzer answers, from ledger evidence:

| Question | Evidence |
|----------|----------|
| Shader encountered / pipeline created? | `ShaderDiscovered`, `CompileStart` within the frame; `PipelineWait` > 0 |
| Cache accessed? | `CacheResolve` on a critical thread (should never happen) |
| Memory allocated / resource uploaded or evicted? | `Alloc`, `Upload`, `Evict` on critical threads |
| Guest CPU blocked? | Guest-core idle time vs. GPU front backlog |
| GPU front/back saturated? | Thread busy ≥ 95% across the frame |
| Synchronization wait? | `FenceWait`, semaphore acquire time |
| Background worker interference? | Worker CPU time overlapping critical threads on the same cores; `ThreadPreempted` |
| Presentation delayed? | `PresentQueued` → `Presented` delta |
| Physical GPU saturated? | GPU busy ≥ 95% with Tier-0 share reported |
| Game's own behavior? | Same spike reproducible with identical guest timing and zero emulator attribution |
| OS/thermal? | Host clock throttling counters, preemption without emulator cause |

A spike is **emulator-induced** if ≥ 50% of the overrun is covered by emulator events on
critical threads. Each class gets a counter; regressions are tracked per class, not overall.

## 5. Statistics honesty

- P99.9 needs roughly 10,000+ frames per sample to be meaningful; at 60 FPS that is ≥ 3
  minutes, and we use ≥ 10-minute runs for P99.9 claims. Shorter runs report up to P99.
- Every comparison is A/B on the same machine, same scenario, ≥ 3 repetitions, with the cache
  state stated (cold/warm), and with variance reported.
- Cold-cache runs delete the title store **and** the driver's own cache where the driver allows
  it (otherwise documented as "driver cache warm").

## 6. Mandatory scenarios

The 20 scenarios of charter §13.1 are implemented as scripted runs (input scripts + capture
replays). Each scenario lists: title, start state, input script, duration, cache state, and
pass criteria. Scenarios that require game content are run privately; captured command streams
are never published (doc 09).

## 7. Proposed quantitative targets (provisional, to be calibrated on reference hosts)

Reference host classes are defined before measurement (e.g. "mid desktop", "low desktop",
"handheld PC APU"); exact models are chosen when the team has the hardware. Targets apply
where the host is not the bottleneck (median frame time ≤ 75% of the interval).

| Metric | 60 FPS title (16.67 ms) | 30 FPS title (33.33 ms) |
|--------|-------------------------|-------------------------|
| Emulator-induced frames > 2× interval, 30-min session, cold cache | 0 | 0 |
| P99 frame time, cold cache | ≤ 1.15 × interval | ≤ 1.10 × interval |
| P99.9 frame time, cold cache | ≤ 1.5 × interval | ≤ 1.35 × interval |
| P99.9 frame time, warm cache | ≤ 1.25 × interval | ≤ 1.2 × interval |
| Pipeline/shader wait on critical path | 0 ms, except no-fallback list (logged) | same |
| Cache-related work on critical threads | 0 I/O; ≤ 0.2 ms CPU per frame (P99.9) | same |
| Cache open, any size | ≤ 10 ms P99 | same |
| Launch → first guest instruction | ≤ 1.5 s (excluding OS cold file cache) | same |

These numbers are design targets, not results. They will be revised after PoC-10 with data.

## 8. Regression gates

- Replay-based CI: deterministic GPU command captures run on a fixed runner; frame-time
  percentiles of the *emulator side* (front/back thread time per frame) are gated.
- Microbenchmarks: cache lookup, resolve, pre-decode, translation per instruction, JIT block
  compile, MME macro execution.
- Any PR touching a critical-thread path must attach before/after ledger summaries.
