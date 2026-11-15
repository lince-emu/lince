# yuzu "Project Prometheus" multicore: kernel scheduler and timing

Task context: T-051 (guest threads on six core threads). Behavior only; no code copied or
read (LEGAL-20/21). Companion to `ryujinx-kernel-scheduler.md`.

STATUS: PARTIAL, documentation only. No yuzu source was read: the official repository is gone
(see "Source access") and unauthorized mirrors are deliberately not used (maintainer,
2026-11-14). Claims rest on the official yuzu blog (DOC) and one recall item (RECALL). Nothing
here is SRC or VERIFIED.

## Question

How did yuzu's Prometheus multicore work run guest cores on host threads, schedule guest
threads, choose host timing over cycle timers, and what did it cost on small hosts?

## Short answer

Prometheus gave each of the 4 Switch cores its own host thread (plus a timer thread and a GPU
thread, 6 in total) and reworked the Citra-derived kernel to follow the Switch OS scheduler more
closely. Guest threads are switched with fibers, not host threads. Cycle timers
(guest-instruction counted ticks) were dropped in multicore because keeping all cores advancing
at the same pace was very hard, so multicore uses the host clock; accuracy had to be reduced for
at least one game. Multicore was an option, not the only mode. Cost: extra memory (100 MB to
3 GB at first release), and a warning against enabling it on 2-core hosts. The blog does not
describe run queues, preemption, yield or migration; for those see the Ryujinx note.

## Source access

- `github.com/yuzu-mirror/yuzu`: HTTP 403 (DMCA takedown notice shown by GitHub, April 2024).
  `github.com/eden-emu/eden`: not found. Not circumvented. No local clone exists.
- Third-party mirrors of the yuzu source are not used as sources (maintainer decision,
  2026-11-14).
- The official blog is still served at `yuzu-mirror.github.io`; all DOC rows cite it.

## Findings

| Source | Observation | Level | Applies to Lince? |
|---|---|---|---|
| yuzu-mirror.github.io/entry/yuzu-prometheus (blog, May 2020) | One host thread per Switch core (4). Total 6 with a timer thread and a GPU thread. Earlier yuzu used at best 2 (CPU, GPU). | DOC | Yes: confirms the "core thread per guest core" shape (ADR-0017); Lince has 6 cores. |
| same | Old kernel used a complex "stops and callbacks" system (Citra heritage); Prometheus reworked scheduling to match the Switch OS more closely. | DOC | Yes: same direction as T-051. |
| same | A kernel-level thread per core hands control between the CPU manager and emulation; JIT is per guest thread rather than per core. | DOC | Partly: per-thread JIT state costs memory; Lince's cache is not per guest thread. |
| same | Guest thread switches use fibers, chosen over host kernel threads because a switch costs "at worst 50 host CPU cycles" (claim as stated by yuzu, not measured by us). To avoid a deadlock, the scheduler frees the current thread and switches through an intermediary. | DOC | Idea only: a guest-thread switch must be user-space on the core thread, never an OS thread handoff. Lince core loops swap register contexts directly. |
| same | Cycle timers count executed guest instructions as ticks (deterministic) but were "very hard to keep all cores advancing at the same pace". yuzu moved to host timing (host clock). | DOC | Key: with 6 free-running core threads, a per-core instruction clock drifts. A shared guest clock must be derived from a host monotonic clock (or a core-0 leader), see Inferences. |
| same | Host timing was too accurate for Hyrule Warriors (soft lock); lowering host timer accuracy to better match hardware fixed it. | DOC | Yes: a purely host-derived clock exposes races the real hardware's coarser timing hides; keep tick rate and timer granularity configurable. |
| same | Initial release "may use additional memory (100 MB to 3 GB depending on the game)", per-thread JITs blamed; fix in stability testing. | DOC | Yes: memory cost scales with per-thread translated code; Lince cache design should avoid per-guest-thread duplicates. |
| same | Audio can be slower in multicore; some games (Super Mario Odyssey) barely use cores 1 and 2 and gain little; GPU still the bottleneck in others. | DOC | Context only. |
| yuzu-mirror.github.io/entry/yuzu-progress-report-may-2020 | Multicore is a user option. Gains aimed at 4+ physical cores; users with 2 cores advised not to enable it (likely slower). Up to 6 or 7 CPU threads in ideal conditions. | DOC | Yes: Lince must degrade (fewer runnable core threads or a single-thread mode) on small hosts; see Open questions. |
| yuzu-mirror.github.io/entry/yuzu-progress-report-jun-2020 | Multicore reached mainline in June 2020, as part of the kernel rewrite (which also added 32-bit ARMv7 support). | DOC | Context. |
| yuzu 2020 slice period and queue rotation rules | Real-kernel preemption (10 ms, one priority per core) is described in the Ryujinx note; yuzu's values are not confirmed. | RECALL | Do not rely; see Ryujinx note. |

## Inferences (guesses, not facts)

- INFERENCE: cycle timers failed across cores because ticks were advanced per core by executed
  instructions, while cores run at different host speeds and block at different times; there
  is no cheap global ordering of events. The blog states the difficulty but not this mechanism.
- INFERENCE: for Lince, a guest clock that is a pure function of the host monotonic clock
  (scaled to the 19.2 MHz style counter) avoids cross-core sync; any deterministic mode would
  need a barrier-based quantum, which costs throughput. Not decided here.
- INFERENCE: yuzu's fibers solve a problem Lince does not have if each core thread simply
  loads the next guest thread's register state and continues in the same interpreter or JIT
  loop (no per-guest-thread host stack).

## Comparison: yuzu vs Ryujinx vs Lince (ADR-0017, six parking core threads)

| Aspect | yuzu (Prometheus, blog only) | Ryujinx (see companion note) | Lince plan |
|---|---|---|---|
| Host threads | 1 per core + timer + GPU; fibers per guest thread | 1 per guest thread + preemption thread | 6 parking core threads; guest threads are data, not host threads |
| Queues, preemption, yield | not described in the blog | per-core scheduled and suggested queues, 10 ms rotation, three yield variants | follow the Ryujinx note's behavior; start with scheduled queues only |
| Time base | host clock (cycle timers abandoned) | host sleep-based 10 ms quantum | host monotonic clock behind one guest-clock abstraction; quantum from core-local timer |
| Single-core | multicore optional; single-core remained the alternative | n/a | consider as a degraded mode on small hosts |
| Costs | memory per-thread JIT; bad on 2 cores | memory/threads per guest thread | no per-guest-thread host stack or JIT; core threads fixed at 6, so small hosts oversubscribe |

## Open questions

1. yuzu's queue, preemption and migration rules are not documented in the blog, and its source
   is not used (Source access). A further doc-only pass over the yuzu progress reports of
   July to December 2020 may add detail.
2. Does Lince need a single-core (one runnable core thread) mode for hosts with fewer than 6
   hardware threads, and should core threads be gated by `lince-sched` rather than the OS?
3. Tick source and granularity of the guest clock (Hyrule Warriors lesson); who owns timer
   events (a timer thread as in yuzu, or per-core timer wheels)?
4. Exact preemption slice and priorities on NX2 remain unmeasured (D-xxx hardware test).

## Affected ADRs, PoCs, backlog

- ADR-0017 (core threads): supports the model; adds the small-host degradation concern.
- T-051: no change to the plan; the scheduler behavior comes from the Ryujinx note.
- Possible new design note or ADR: guest clock derivation and timer thread ownership.
- Related: `docs/research/ryujinx-kernel-scheduler.md`, `docs/research/cpu-jit-scheduling.md`.
