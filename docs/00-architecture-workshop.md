# 00 — Architecture Workshop: Charter Review

Status: Living document · Last reviewed: 2026-11-13

This is the entry point of the Lince engineering dossier. It reviews the project charter
before any subsystem is designed, and records what is confirmed, what is open, what conflicts,
and what must be prototyped.

Lince is written **from scratch**. Existing emulators are studied to understand their
decisions and failure modes, never to inherit their architecture. The defining goal is a
different approach to guest shaders:

> **No gameplay stutter caused by shader or pipeline compilation, and no shader loading or
> precompilation before the game starts — with a completely empty cache.**

## 1. Confirmed requirements

| ID | Requirement | Source |
|----|-------------|--------|
| R1 | Implementation language is Rust. | Charter §2.1 |
| R2 | Switch 2 first; Switch 1 later in the same project; no compromise of Switch 2 design for early Switch 1 support. | §2.2 |
| R3 | Independent implementation; no forks of yuzu/Ryujinx/Eden/Eglantine/Lichii. | §2.3 |
| R4 | No proprietary keys or firmware as architectural dependencies; original HLE. No key extraction, no encryption circumvention. | §2.4 |
| R5 | Consistent frame delivery is the primary objective, measured by frame-time distribution, not average FPS. | §3 |
| R6 | Empty cache is a fully supported mode; no mandatory precompilation, warm-up, restart or cache download. | §3.1 |
| R7 | Never discard draws, dispatches, resource updates or guest-visible side effects to hide missing work. | §3.3 |
| R8 | No full cache load/deserialize/validate at game startup; startup time independent of cache size. | §4.1 |
| R9 | A cache miss must not block gameplay when a correct fallback exists. | §4.2, §7 |
| R10 | Background work must be governed so it cannot create stutter. | §8 |
| R11 | Every proposal has a measurement plan; no invented hardware facts or benchmark numbers. | §13, §15 |

## 2. Open technical decisions (tracked as ADRs)

Everything except Rust is open. See [`adr/README.md`](adr/README.md) for the full index. The
decisions with the largest blast radius are:

1. **Tier-0 shader execution strategy** (GPU interpreter vs. generated ubershaders vs. fast
   baseline compile) — [ADR-0010](adr/0010-shader-execution-fallback.md). Cannot be finalized
   without PoC-2/PoC-3.
2. **Cache storage engine** (custom content-addressed segment store vs. embedded database) —
   [ADR-0015](adr/0015-cache-persistence-format.md). Needs PoC-6.
3. **Host graphics API and capability tiers** — [ADR-0007](adr/0007-host-graphics-api.md).
4. **Shader IR designed for Ampere semantics** (not a Maxwell IR) — [ADR-0009](adr/0009-shader-ir.md).
5. **GPU command pipeline split across threads from day one** — [ADR-0008](adr/0008-guest-gpu-command-architecture.md).
6. **Project license** — [ADR-0000](adr/0000-project-license.md). Must be decided before the
   first line of code because it constrains what may be read during clean-room work.

## 3. Unverified assumptions in the charter

| Assumption | Why it is unverified | How we verify |
|------------|----------------------|---------------|
| A correct generic shader path exists for *every* guest shader class. | Tensor-core (HMMA) networks, task/mesh shaders, and very large compute programs are known to exist in Switch 2 titles (Lichii `NX2_NOTES.md` rows 64, 89, 121). A GPU interpreter for them may be impossible or too slow. | PoC-2 measures interpreter coverage and cost per instruction class. |
| A GPU interpreter's runtime cost fits inside a frame. | Interpretation can be one to two orders of magnitude slower than native code (hypothesis; no Lince data yet). If many new shaders appear at once, the *GPU* becomes the stutter source. | PoC-2/PoC-10 measure fallback GPU time per draw and per frame. |
| Driver pipeline creation can be fully moved off the critical path. | Even with a generic shader, Vulkan needs a pipeline (or shader objects) per fixed-function state class. | PoC-5 measures pipeline count after maximal dynamic state, and the link cost with graphics pipeline libraries. |
| Shader translation can run on worker threads. | Lichii found its translation reads live GPU state (texture types, constant buffer values) and kept it on the GPU thread (`NX2_NOTES.md` row 121). | Lince IR records *dependencies* explicitly (ADR-0009) so translation consumes a snapshot. |
| A warm cache will remove all hitches. | Lichii's hitches also come from texture churn, buffer downloads at fences, lock contention, macro execution and a saturated GPU thread (rows 99–145). | Frame ledger attribution (doc 05) classifies every spike. |

## 4. Contradictions and tensions

1. **"Draws must not wait" + "never discard draws" + "no precompilation"** cannot all hold for a
   shader that has no correct generic path. Resolution: **correctness wins**. The draw waits,
   but only for shader classes on an explicit, measured *no-fallback list*, and the scheduler
   gives those compiles absolute priority. Every such wait is logged as a defect to close,
   not as accepted behavior. Discovery-time compilation (doc 03 §5) is the main mitigation.
2. **Immediate correct execution vs. GPU budget.** The fallback tier moves cost from the CPU
   (compile) to the GPU (slow execution). Tier-up must be prioritized by *measured fallback GPU
   cost*, not by first-seen order.
3. **"Rust only" vs. unavoidable foreign code.** The Vulkan loader and drivers are C; video
   codecs (VP9/H.264, observed in Switch 2 titles, Lichii rows 105/148) have no mature pure-Rust
   implementation. Policy: foreign code allowed only behind a crate boundary, never in the
   emulation core, and each instance needs an ADR ([ADR-0026](adr/0026-foreign-code-policy.md)).
4. **"Initial target Switch 2" vs. "no keys".** Retail content is encrypted. Lince loads
   only content the user has already obtained and decrypted by lawful means, and does not ship
   decryption of protected formats ([`09-legal-and-content.md`](09-legal-and-content.md)).
5. **"P99.9 frame time" vs. session length.** At 60 FPS, a stable P99.9 needs on the order of
   tens of thousands of frames (≥ 10 minutes). Short captures cannot report P99.9 honestly
   ([`05-performance-validation.md`](05-performance-validation.md)).

## 5. Missing architectural components (added to the dossier)

- Guest-state **dependency tracking** for shader variants (what a translation read, so it can be
  keyed and replayed off-thread).
- **Discovery-time shader compilation**: detect new shader code when the guest *uploads* it, not
  when it first draws with it.
- A **frame ledger**: always-on, low-overhead per-frame event log that correlates every
  operation with the frame deadline it affected.
- **GPU command stream capture & replay** for deterministic performance regression tests.
- **Video decoding** (VP9/H.264), **audio**, **input**, **save data**, and **host thread-priority
  management**, which the charter lists only implicitly.
- **License and clean-room process** (affects every contributor, including AI assistants).

## 6. Critical technical risks

| Risk | Impact | Mitigation | PoC |
|------|--------|-----------|-----|
| GPU interpreter infeasible or too slow for fragment shaders (derivatives, divergence). | Tier 0 collapses; first-use stalls return. | Explicit quad derivative emulation; per-class fallback list; discovery-time compile. | PoC-2 |
| Ampere SASS ISA knowledge is incomplete (undocumented by NVIDIA). | Translation gaps → missing rendering. | Differential testing against real SASS compiled from our own CUDA sources; nvdisasm as an oracle for decoding only. | PoC-4 |
| Tensor-core (DLSS-style) networks. | Correct execution may be seconds per frame (Lichii row 64). | Cooperative-matrix tier on capable hosts; correct slow path; labelled, opt-in approximation. | Later |
| GPU command processing is CPU-bound on one thread. | Stutter and low FPS independent of shaders (Lichii: GPU thread ~100%, host GPU 20–40%). | Pipelined front/back design from day one (ADR-0008). | PoC-9 |
| Custom cache store has corruption bugs. | Crashes or wrong rendering. | Every artifact is checksummed and re-derivable; the cache is never trusted. | PoC-6 |
| Driver variance (pipeline libraries, shader objects, pipeline binaries). | Uneven experience across vendors. | Capability tiers with tested fallbacks (ADR-0007, ADR-0013). | PoC-5 |

## 7. Research gaps

- Ryujinx current source (`git.ryujinx.app`) was not reachable from the research environment;
  findings use a public mirror of older tags and are marked accordingly.
- yuzu's original repository is offline; its pipeline cache design was studied through its
  descendants (Lichii/Eglantine `vk_pipeline_cache.cpp`) — those files are modified and must not
  be read as original yuzu behavior without a note.
- NVN2 graphics semantics are known only through reverse engineering in emulator notes; no
  primary documentation is available.
- Oboromi, Pound, Hokkaido, Anka, Orchid and Eden were inspected later in T-015; see
  `docs/research/switch2-emulator-projects.md`.

## 8. Subsystem review summary

| Subsystem | Confirmed | Recommended | Unresolved | Prototype | ADRs ready to accept |
|-----------|-----------|-------------|-----------|-----------|----------------------|
| Frame-time consistency | R5, R10 | Frame ledger + governor | Exact budgets per host tier | PoC-7, PoC-8, PoC-10 | 0023 (frame pacing principles) |
| Cache | R6, R8 | Content-addressed segments + mmap index, lazy resolve off render thread | Store engine | PoC-6 | 0014 (principles) |
| First-run shaders | R6, R9 | Tier 0 GPU interpreter + discovery-time compile | Interpreter viability | PoC-1, PoC-2 | — |
| Ubershaders | — | Not the primary tier (see doc 03) | Hybrid use for fixed-function-like shaders | PoC-3 | — |
| Specialized compile | — | Lince IR → SPIR-V, stage libraries, background LTO | IR details | PoC-4, PoC-5 | 0009 (Ampere-native IR) |
| CPU | — | Interpreter + baseline JIT + async optimizing tier | JIT backend crate | — | 0005 (tiering principle) |
| GPU command | Lichii evidence | Front/back pipeline, macro JIT | Back-end parallel recording | PoC-9 | 0008 |
| Scheduling | R10 | Dedicated threads, priority classes, governor | Policy constants | PoC-8 | 0017 |
| Memory/resources | — | Fastmem, GPU page table, sparse huge textures | Windows placeholder APIs | — | — |
| Presentation | — | Present-wait pacing, bounded fences | VRR policy | PoC-10 | 0023 |
| HLE | R4 | Original services; IPC framework | Switch 2 service protocols | — | 0024 |
| Build | R1 | Cargo workspace + xtask | Toolchain pin | — | 0001, 0002 |
