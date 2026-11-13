# 06 — Proof-of-Concept Roadmap

Status: Proposed · Order reflects risk: the PoCs that can invalidate the architecture come first.

Each PoC is throwaway-quality code in `poc/<name>/` unless promoted by an ADR. Every PoC ends
with a short report in `docs/poc-reports/` that updates the ADRs it enables.

| Order | PoC | Decides | Depends on |
|-------|-----|---------|------------|
| 1 | PoC-2 GPU interpreter | ADR-0010 | — |
| 2 | PoC-6 Cache store | ADR-0015, 0016 | — |
| 3 | PoC-5 Pipeline mechanisms | ADR-0007, 0013 | — |
| 4 | PoC-7 Frame ledger | ADR-0027 | — |
| 5 | PoC-4 Ampere decoder + IR + SPIR-V | ADR-0009, 0012 | — |
| 6 | PoC-3 Program-generic Tier 1 | ADR-0011 | PoC-4 |
| 7 | PoC-9 GPU command pipeline | ADR-0008 | Command captures (M4) |
| 8 | PoC-8 Governor | ADR-0018 | PoC-7 |
| 9 | PoC-1 Empty-cache first run | Charter feasibility | M4–M8 |
| 10 | PoC-10 Continuous stability | ADR-0023, targets of doc 05 | Everything above |

---

## PoC-1 — Empty-cache first-run gameplay

- **Objective:** Show a session from an empty cache with zero pipeline waits on the critical path.
- **Hypothesis:** With Tier 0 + discovery-time compile, emulator-induced frames > 2× interval
  are zero in the first 30 minutes; most programs are uploaded ≥ several frames before first draw.
- **Scope:** One title end-to-end (boot to gameplay), ledger on.
- **Alternatives compared:** Tier 0 on/off; discovery-time compile on/off.
- **Correctness tests:** Frame image comparison with Tier 0 forced vs. Tier 2 forced.
- **Benchmark method:** Scenarios 1, 4, 7, 19 of doc 05; ≥ 3 cold runs each.
- **Success:** Zero pipeline waits outside the no-fallback list; P99 within doc 05 targets.
- **Failure:** Any skipped draw; any wait outside the list; P99.9 > 2× interval attributable to Tier 0 GPU cost.
- **Enables:** Confirms or revises the charter-level guarantee.

## PoC-2 — Generic shader execution (GPU interpreter)

- **Objective:** Prove a GPU interpreter of SASS micro-ops is correct and affordable.
- **Hypothesis:** Compute and fragment programs run bit-exact vs. a compiled reference at a GPU
  cost factor acceptable for short exposure (factor to be recorded, not assumed).
- **Scope:** Micro-op format; interpreter for ALU, control flow, LDG/STG/ATOM, shared memory,
  barriers, SHFL/VOTE, TEX with explicit derivatives, IPA via barycentrics; standalone Vulkan
  harness (no emulator).
- **Alternatives:** Raw SASS interpreted directly vs. pre-decoded micro-ops; register-file
  buckets vs. single 256-register file; host subgroup 32 vs. shared-memory warp emulation.
- **Correctness tests:** Corpus of SASS compiled from team-written CUDA kernels (sm_86) with
  known outputs; hand-assembled fragment-like programs; comparison vs. Tier-1 output.
- **Benchmark:** GPU timestamps per dispatch/draw; interpreter pipeline compile time per vendor.
- **Success:** Bit-exact on corpus; cost factor documented per instruction class; derivatives
  correct under divergence.
- **Failure:** Fragment derivatives or helper lanes cannot be made correct on V2/V3 hosts.
- **Enables:** ADR-0010.

## PoC-3 — Program-generic (ubershader-style) Tier 1

- **Objective:** Measure compile-count reduction from treating dependencies as runtime inputs.
- **Hypothesis:** Host compiles ≈ distinct guest programs (not program × state).
- **Scope:** Recorded sessions' dependency sets; Tier-1 generator with parameter blocks.
- **Alternatives:** Fully specialized vs. program-generic.
- **Correctness:** Differential vs. specialized output.
- **Benchmark:** Compiles per minute; GPU overhead per draw.
- **Success:** ≥ 3× fewer compiles with ≤ 25% GPU overhead on Tier 1.
- **Failure:** Overhead > 25% or no compile reduction.
- **Enables:** ADR-0011.

## PoC-4 — Specialized shader generation

- **Objective:** Ampere SASS → Lince IR → SPIR-V that validates and matches reference results.
- **Hypothesis:** The IR expresses all corpus instructions without lowering to Maxwell.
- **Scope:** Decoder (instruction table), IR, structurizer, SPIR-V emitter, offline `lince-shader-lab`.
- **Alternatives:** Structurization algorithms.
- **Correctness:** `spirv-val`; bit-exact per opcode vs. CUDA reference outputs; fuzzing.
- **Benchmark:** Translation time per instruction (P50/P99).
- **Success:** 100% of corpus translates; zero mismatches.
- **Failure:** Structurizer fails on real control flow without a workaround.
- **Enables:** ADR-0009, ADR-0012.

## PoC-5 — Non-blocking pipeline creation

- **Objective:** Measure each Vulkan mechanism per vendor.
- **Hypothesis:** Fast-link / shader-object creation is ≤ 1 ms; LTO and full compiles are
  orders of magnitude slower; pipeline-binary loads are sub-millisecond.
- **Scope:** Standalone harness with generated SPIR-V of realistic sizes.
- **Alternatives:** Monolithic, GPL (non-LTO/LTO), shader objects, pipeline binary, sharded
  `VkPipelineCache`, `DISABLE_OPTIMIZATION`.
- **Correctness:** Validation layers clean; identical rendering.
- **Benchmark:** Creation latency distributions; thread scalability (1–8 workers).
- **Success:** At least one mechanism per tier-1 vendor meets the fast-link hypothesis.
- **Failure:** No vendor-wide fast path → Tier 0 coverage must be broader.
- **Enables:** ADR-0007, ADR-0013, driver override table.

## PoC-6 — Persistent cache indexing and lazy access

- **Objective:** Validate O(1) open and fast resolves; pick the storage engine.
- **Hypothesis:** Custom store opens in ≤ 10 ms P99 for 1 MiB–4 GiB with ≤ 2 ms difference.
- **Scope:** `lince-cache` prototype; redb and SQLite comparators; synthetic workload
  (blob size distribution from Lichii-like caches).
- **Alternatives:** Custom, redb, SQLite WAL.
- **Correctness:** Fault injection (kill -9 during appends, bit flips, truncation, concurrent
  reader process).
- **Benchmark:** Open time, resolve P99.9 under concurrent writes, recovery time, space
  amplification; cold OS cache on NVMe and HDD.
- **Success:** Targets met; no corruption escapes as wrong data.
- **Failure:** Startup grows with size, or any corruption returns wrong bytes.
- **Enables:** ADR-0015, ADR-0016.

## PoC-7 — Frame ledger (enabler for PoC-8/10)

- **Objective:** Always-on event logging under 1% overhead.
- **Scope:** Per-thread rings, analyzer, Tracy bridge.
- **Success:** Event cost ≤ 30 ns; analyzer finds injected synthetic spikes with correct cause.
- **Enables:** ADR-0027.

## PoC-8 — Background compilation under frame-time pressure / adaptive scheduling

- **Objective:** Show the governor keeps critical threads unaffected under heavy compile load.
- **Hypothesis:** A governed pool beats any fixed worker count on P99.9 while keeping Tier-0
  exposure similar.
- **Scope:** Synthetic emulator load (busy critical threads with deadlines) + real compile jobs.
- **Alternatives:** Fixed N workers (N = 1..cores), OS priorities only, governor.
- **Correctness:** All jobs eventually complete; cancellation works.
- **Benchmark:** Deadline misses, critical-thread run-queue delay, compile throughput on 6-, 8-,
  16-thread hosts.
- **Success:** Governor ≥ best fixed policy on ≥ 2 host classes.
- **Failure:** Oscillation or starvation not fixable by tuning.
- **Enables:** ADR-0017, ADR-0018.

## PoC-9 — CPU/GPU execution overlap (GPU command pipeline)

- **Objective:** Front/back/record pipeline throughput on recorded command streams.
- **Hypothesis:** ≥ 100,000 draws/s on a mid host; sync points don't collapse overlap.
- **Scope:** Pushbuffer parser, MME64 interpreter + memo, state tracker, packet builder, null backend.
- **Alternatives:** Single thread vs. 2-stage vs. 3-stage.
- **Correctness:** Mirror verification mode; packet stream equality single vs. pipelined.
- **Benchmark:** Draws/s, stage busy %, sync points/s.
- **Success:** ≥ 1.4× over single thread (Lichii measured 1.46×) with headroom.
- **Enables:** ADR-0008.

## PoC-10 — Continuous frame-time stability

- **Objective:** 30-minute sessions across all 20 scenarios meet doc 05 targets.
- **Scope:** Full emulator at M11.
- **Success/Failure:** doc 05 §7 table.
- **Enables:** Calibrated targets; ADR-0023 acceptance.
