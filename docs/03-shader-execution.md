# 03 — Shader Execution Architecture

Status: Proposed · Related ADRs: 0009, 0010, 0011, 0012, 0013 · Validating PoCs: PoC-1 to PoC-5

## 1. The problem, stated precisely

A guest draw references guest shader *code* (Ampere SASS) that the emulator may never have
seen. Today's Switch emulators translate it, hand the result to the host driver, and wait for
the driver to compile — or skip the draw. Both are unacceptable for Lince-NX.

The vision: **separate "running a shader correctly" from "compiling a shader".** Compilation
is an optimization that happens in the background. Correct execution is available from the
first draw, without any host compile that depends on the guest program.

Three costs must be kept apart:

| Cost | Where it happens | Who can hide it |
|------|------------------|-----------------|
| Guest discovery & decode | Emulator CPU | Discovery-time work, caching |
| Guest → host translation | Emulator CPU | Worker threads, if dependencies are explicit |
| Host driver compile (SPIR-V → GPU ISA, pipeline creation) | Driver, on whichever thread calls it | Only by *not needing it* for first use |

The last row is the root cause of compile stutter. Only a data-driven execution path — where
the guest program is *data* consumed by an already-compiled host shader — removes it.

## 2. Candidates compared

| Criterion | A. GPU interpreter | B. Generated ubershaders | C. Hybrid (generic now, specialized later) | D. Multi-tier | E. Fast baseline compile only |
|-----------|--------------------|--------------------------|--------------------------------------------|---------------|-------------------------------|
| First-use latency | **No host compile** per guest program | A compile per ubershader family (must exist before use) | Depends on generic path | Same as best tier available | Driver compile (ms–s) on first use |
| Runtime overhead | High (hypothesis: 5–50× ALU; to measure) | Medium (branching on parameters) | Temporary | Decreases over time | Low–medium |
| Compile overhead | One-time, global (per driver) | Per family | Background | Background | On critical path unless hidden |
| Memory | Small (micro-ops) + large register files (scratch) | Large shaders | Both tiers | All tiers | Low |
| Control-flow divergence | Severe (interpreter loop) | Moderate | — | — | Native |
| Texture ops / derivatives | Needs explicit quad derivatives | Native | — | — | Native |
| Warp/subgroup semantics | Must emulate 32-wide warps | Native-ish | — | — | Native-ish |
| Compute + shared memory + barriers | Feasible with phased loop (§4.4) | Native | — | — | Native |
| Correctness risk | Two implementations of semantics | Combinatorial | Tier mismatch | Tier mismatch | Low |
| Portability | Needs descriptor indexing, BDA, subgroups | Good | — | — | Good |
| Complexity | High | Very high for arbitrary code | High | Highest | Low |

**Why classic ubershaders (B) are not the primary answer for Switch 2:** Dolphin's ubershader
interprets a bounded *configuration* space (TEV). Switch 2 shaders are unbounded *programs*.
An ubershader that can run any program is, by definition, an interpreter (A). B remains useful
in a narrow role: one host shader per guest program that takes its state dependencies as
runtime parameters (§4.2), so a program compiles once regardless of state.

**Why E alone fails the charter:** with E the first draw of every new program waits for the
driver, or must be skipped. Driver compile times of tens to hundreds of milliseconds per
pipeline are common on desktop drivers [hypothesis to quantify in PoC-5].

## 3. Recommendation: a three-tier model with discovery-time compilation

```text
             guest uploads code            first draw                  background
                    │                          │                           │
   ┌────────────────▼─────────┐   ┌────────────▼──────────┐   ┌────────────▼─────────────┐
   │ Discovery                │   │ Tier 0: GPU interpreter│   │ Tier 1: baseline native  │
   │ scan, hash, pre-decode   │──▶│ global pipelines,      │──▶│ program-generic SPIR-V,  │
   │ to micro-ops, queue T1   │   │ micro-ops as data      │   │ stage libraries / shader │
   └──────────────────────────┘   │ ZERO per-program compile│  │ objects, fast link       │
                                  └────────────────────────┘   └────────────┬─────────────┘
                                                                            ▼
                                                               ┌──────────────────────────┐
                                                               │ Tier 2: optimized        │
                                                               │ specialized constants,   │
                                                               │ LTO pipeline, persisted  │
                                                               │ driver binary            │
                                                               └──────────────────────────┘
```

| Tier | Purpose | Produced by | Selected when |
|------|---------|-------------|---------------|
| 0 | Correct execution with no per-program host compile | Global interpreter pipelines (compiled once per driver) + per-program micro-op data | Nothing faster is ready |
| 1 | Native speed, compiled quickly | Lince IR with light passes → SPIR-V; dependencies as parameters; `VK_EXT_graphics_pipeline_library` fast-link or `VK_EXT_shader_object`; optionally `DISABLE_OPTIMIZATION` | Ready; or Tier 0 unsupported for the program class |
| 2 | Best runtime performance | Full optimization, dependencies baked in, LTO link; stored as driver binary | Ready |

Tier selection happens per pipeline slot at draw time by reading one atomic pointer.

**This recommendation is provisional.** It holds only if PoC-2 shows Tier 0 is correct and
affordable for the program classes that matter. If Tier 0 fails for a class, that class goes on
the *no-fallback list* (§6), handled by discovery-time Tier-1 compilation and, as a last
resort, a bounded, logged wait — never a skipped draw.

## 4. Tier 0 — GPU interpreter design (hypotheses to prove)

### 4.1 Input: micro-op stream

A CPU pre-decoder converts SASS into a compact, fixed-width micro-op format (opcode, operand
descriptors, immediates, resolved branch targets, register-file size). The GPU interpreter
reads micro-ops from a storage buffer. Goals: pre-decode is linear and allocation-free, so it
can run at discovery time or, if needed, on the render path with a bounded cost.

### 4.2 Program-generic parameters

Everything Tier 1/2 would bake in from guest state (texture types, formats, cbuf-derived
constants, viewport transform state) is read by the interpreter at runtime from per-draw
parameter blocks. The same idea gives Tier 1 its "compile once per program" property.

### 4.3 Resources

- **Textures/samplers:** global descriptor heap with `descriptorIndexing` (or
  `VK_EXT_descriptor_buffer`), arrays per image dimensionality and component type; the guest's
  bindless handle (from a constant buffer) indexes it after translation through a handle table.
- **Global memory (LDG/STG/ATOM):** buffer device address. Guest GPU virtual addresses are
  translated by a GPU-resident page table (shared with Tier 1/2).
- **Constant buffers:** bound through the same address mechanism.
- **Register file:** up to 255 GPRs + uniform registers + predicates per thread. Implemented as
  local arrays; a few precompiled register-file size buckets (e.g. 32/64/128/256) avoid
  allocating 255 registers for small programs.

### 4.4 Control flow, warps and barriers

- Per-lane program counters emulate Volta+ independent thread scheduling naturally.
- **Reconvergence:** each iteration executes the minimum PC among active lanes of the
  (emulated) warp; this keeps quads together for derivatives and models `BSSY/BSYNC`.
- **Subgroup ops (`SHFL`, `VOTE`, `MATCH`, `REDUX`):** map to host subgroup operations when the
  host subgroup size can be forced to 32 (`VK_EXT_subgroup_size_control`); otherwise emulate
  through workgroup shared memory (compute) — open question for fragment stages.
- **Barriers (`BAR.SYNC`) in compute:** a phased loop — lanes run until they hit a barrier or
  exit, then the loop executes `barrier()` in uniform control flow, then continues.

### 4.5 Graphics-stage interfaces

- **Vertex input:** vertex pulling through device addresses; no vertex-input pipeline state.
- **Varyings and interpolation:** SASS interpolates explicitly (`IPA` with modes). With
  `VK_KHR_fragment_shader_barycentric` the interpreter fetches per-vertex values and performs
  any interpolation mode itself, so one fragment interpreter covers all modes. Without it,
  interpolation-mode variants must be precompiled (fallback).
- **Derivatives / implicit LOD:** computed explicitly with quad operations on the coordinates,
  then `textureGrad`. Helper invocations must be kept alive.
- **Fragment outputs:** the attachment numeric type (float/sint/uint) must match the declared
  output. Interpreter variants are keyed by the *numeric-class vector* of the bound attachments;
  the all-float class is built first. Other classes are built on first sight (global, once per
  driver) — PoC-2 measures how many classes real titles use.

### 4.6 Global interpreter pipelines

The interpreter shaders are emulator assets, identical for every game. They are compiled once
per driver identity in the background on the **emulator's** first launch and stored in
`_global` (doc 02 §2). They rely heavily on dynamic state (dynamic rendering, extended dynamic
state 1/2/3, or shader objects) so their count stays small. This is not a per-game warm-up and
does not delay game start; until the first set is ready (first emulator launch only), Tier 1
compiles serve as the path — an explicitly documented one-time exception to measure.

### 4.7 Cost control

Tier 0 moves cost from the CPU to the GPU. Each Tier-0 draw is timed with GPU timestamps
(sampled), and the measured cost × frequency sets the Tier-1 compile priority. A program that is
expensive under Tier 0 jumps the queue.

## 5. Discovery-time compilation

Games upload shader code to GPU memory before first use — typically during loading screens.
Lince-NX watches writes into the guest's shader program region (and NVN2 program-setup paths
once they are understood) and:

1. Scans new code for program headers, computes `GuestProgramId`, pre-decodes micro-ops.
2. Queues Tier-1 compilation of stage libraries at P2 (speculative) priority.
3. Records the program in the session profile.

The loading screens the game already has become the compile window — no emulator-imposed
waiting. Hypothesis to validate (PoC-1): a large fraction of programs are uploaded more than a
few frames before their first draw.

## 6. The no-fallback list

Program classes for which Tier 0 is not (yet) correct or affordable, initially expected to
include: tensor-core `HMMA` programs, task/mesh programs, and programs over a size limit
measured in PoC-2. For these:

- Discovery-time Tier-1 compile at **P0**.
- If a draw arrives before it is ready, the draw **waits** (correctness wins), the wait is
  recorded in the frame ledger as an emulator-induced stall, and the class is tracked as a
  defect.
- Approximations (e.g. replacing a tensor-core upscaler) are opt-in, labelled in the UI and
  logs, and never the default.

## 7. Specialized compilation path (Tier 1 / Tier 2)

1. Guest discovery → 2. SASS decode (Ampere-native) → 3. Lince IR (SSA, predicates, uniform
datapath, convergence barriers) → 4. passes (Tier 1: minimal; Tier 2: full) → 5. SPIR-V emission
→ 6. shader module / stage library / shader object → 7. pipeline layout (shared, global) →
8. pipeline link (fast) or LTO pipeline (Tier 2) → 9. driver compile → 10. descriptor
preparation → 11. atomic publication.

- Steps 1–2 at discovery; 3–11 on compile workers. None on the render-critical path.
- Pipeline layouts are global and fixed (bindless model), so layout creation never happens per
  pipeline.
- Translation reads only the immutable dependency snapshot taken at request time, never live
  GPU state (lesson E9 in doc 01).

## 8. Tier transitions and visual consistency

- Switching tiers mid-scene must not cause visible flicker. Both tiers implement identical
  arithmetic: no FMA contraction unless the guest instruction is an FMA (`NoContraction`),
  identical denormal and NaN handling, identical special-function precision where possible.
- Differential tests (PoC-4) compare Tier 0, Tier 1 and Tier 2 outputs bit-for-bit on captured
  draws; any difference is a bug in one of them.

## 9. Prototypes that decide this architecture

| PoC | Question |
|-----|----------|
| PoC-1 | What does an empty-cache session look like end to end, and when are programs uploaded vs. first drawn? |
| PoC-2 | Is a GPU SASS interpreter correct and affordable per stage and instruction class? |
| PoC-3 | Do program-generic Tier-1 shaders (dependencies as parameters) cut compile counts enough to matter? |
| PoC-4 | Ampere decoder + IR + SPIR-V correctness via differential tests. |
| PoC-5 | Per-vendor cost of stage libraries, fast link, shader objects, pipeline binaries; resulting pipeline counts. |
