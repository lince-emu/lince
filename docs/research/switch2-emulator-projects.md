# Survey of other Switch 2 projects: oboromi, Pound, Hokkaido, Anka, Orchid, Eden

- **Task:** T-015 (follows doc 01 §8 "Not yet inspected").
- **Research date:** 2026-11-13.
- **Method:** shallow, sparse clones in a scratch directory outside the Lince tree, plus the
  GitHub and Forgejo APIs for metadata. For the four large C++ forks only top-level Markdown,
  `docs/`, file listings and the header comments of a few cache files were read. No code was
  copied. Nothing in this note is transcribed from source; it describes behavior and design.
- **Evidence levels:** as in doc 01 §0 — SRC, DOC, DOC-MEASURED, MIRROR, RECALL, VERIFIED.
  "SRC (listing)" means the claim rests only on the presence or absence of file paths at the
  pinned commit, not on reading the file. "SRC (comments)" means only the header comments of
  the named files were read.
- **Applies to Lince:** yes / no / partially, with a reason.

## 1. Question

What do the other public Switch 2 projects use for language, CPU backend, shader approach and
cache design? Are any Rust crates worth evaluating? Which of their lessons should change Lince's
ADRs or PoCs, and which sources must Lince avoid?

## 2. Short answer

Only **oboromi** (Rust; Unicorn CPU; direct SASS→SPIR-V; no cache) and **Pound** (C; waiting on
its own Ballistic JIT) are independent, and neither runs commercial games. **Orchid, Anka and
Hokkaido** are C++ forks of **Eden-NX2** (dynarmic/NCE, Ampere→Maxwell lowering); Eden `master`
has no Switch 2 code. Useful lessons: a "light draw" fast path, cache records that store guest
environments instead of SPIR-V, and gated tensor-core paths. The only Rust code worth studying
is `rapid-arm-emu`. **Legal:** oboromi's SASS decoder tables are off-limits pending D-006 (§5).

## 3. Projects and lineage

| Project | Repository · branch · commit (date) | Language / license | Lineage | Status at commit |
|---|---|---|---|---|
| oboromi | github.com/0xNikilite/oboromi · `main` · `c97664a8460e` (2026-11-03) | Rust core + C++ Qt 6 debug GUI / GPL-3.0 | Independent | README: "cannot currently run commercial games or firmware" |
| rapid-arm-emu (oboromi's planned CPU) | github.com/0xNikilite/rapid-arm-emu · `main` · `d762465dfd05` (2026-08-02; last push 2026-08-13) | Rust / GPL-3 | Independent | JIT core and decoder present; guest front end is a stub |
| Pound | github.com/pound-emu/pound · `main` · `0bbb58e85a32` (2026-11-12) | C / GPL-3.0 | Independent; README says parts reuse yuzu components | Development "fully shifted" to Ballistic |
| Ballistic (Pound's CPU) | github.com/pound-emu/ballistic · `main` · `de07fcaa7708` (2026-10-27) | C / LICENSE file is GPL-2.0 text ("or later" not confirmed) | Described as a rewrite of dynarmic | Tier 1 x86-64 backend only |
| Eden (upstream) | git.eden-emu.dev/eden-emu/eden · `master` · `63e957b1cad6` (2026-11-13) | C++ / GPL-3.0-or-later | yuzu → Sudachi → Eden | Switch 1 only |
| Eden-NX2 | git.eden-emu.dev/Takoroka/eden-nx2 · `eden-nx2` · `50fa86bbbc40` (2026-10-29) | C++ / GPL-3.0-or-later | Forgejo fork of Eden | Origin of `NX2_NOTES.md` / `NX2_GPU_THREAD_SPLIT.md` |
| Orchid | github.com/MythosOfs/Orchid · `main` · `e9d4b5b795e5` (2026-11-12) | C++ / GPL-3.0 | README: "started from Eglantine" | Per-title fixes for a list of target games |
| Anka | github.com/EddizEge/Anka · `anka` · `140d784a7e4d` (2026-11-11) | C++ / GPL-3.0 | From "PokeDinner-Eglantine 2.0.0"; ships Lichii as a second core for one title | Windows, performance-focused |
| Anka Native | github.com/EddizEge/AnkaNative · `main` · `09bf66ea6200` (2026-11-09) | C / Python / MIT (app as a whole GPL-3.0-or-later) | nxnative → Banana Native; not an emulator | Per-title native execution on Android |
| Hokkaido | github.com/pavelgajdo/Hokkaido · `nx2` · `32c4089558e2` (2026-11-11) | C++ / GPL-3.0 | GitHub fork of zortos293/Lichii (formerly "Lichii for Android") | Android, Exynos/Xclipse (RADV) |

Lineage evidence: `NX2_GPU_THREAD_SPLIT.md` has the same git blob hash (`75fd0e41…`) in Eden-NX2,
Orchid and Hokkaido. Anka's copy has grown to 451 lines. Each fork's `NX2_NOTES.md` is a superset
of Eden-NX2's, with Orchid adding 44 unique lines, Anka 204 and Hokkaido 17 [SRC]. Hokkaido's
parent comes from the GitHub API `parent` field [SRC]. "Anka" names two things: the emulator and
the separate Anka Native.

### 3.1 Per-project summary

"Not inspected" means the item was not read at the pinned commit; nothing is guessed. Details
and sources are in §4.

| Project | Language | CPU backend | Shader approach | Cache design | Rust crates of interest |
|---|---|---|---|---|---|
| oboromi | Rust (+ C++ Qt GUI) | Interpreter/emulator via `unicorn-engine` 2.1 (Rust bindings); `dynarmic` fork present as submodule, not wired; `rapid-arm-emu` planned (O1) [SRC] | Direct SASS→SPIR-V, one emitter per opcode, no IR, hand-written word emitter, `ash` compute only; unseen shaders: no handling beyond translate-on-use (O2) [SRC] | None: no file path matching "cache" outside vendored ImGui (O2) [SRC (listing)] | `ash`, `unicorn-engine` (dev/CI oracle only) |
| rapid-arm-emu | Rust | JIT: SSA IR → Cranelift (two tiers), optional `libgccjit` (O4) [SRC] | n/a (CPU only) | Not inspected | `cranelift`, `roxmltree`, `loom`, `criterion` |
| Pound | C | Unicorn plus Ballistic engine interfaces; Ballistic is a C JIT with a structured-SSA IR and an x86-64 Tier 1 (P1, P3) [SRC + DOC] | Planned SM86→SPIR-V with a structure-of-arrays IR; not implemented (P2) [DOC] | JIT code-cache allocator planned, not done; no shader cache (P1) [DOC] | none (C project) |
| Eden (`master`) | C++ | `dynarmic` (in-tree `src/dynarmic/`) plus NCE on ARM64 hosts [SRC (listing)] | Switch 1 Maxwell recompiler; no Switch 2 paths (E-1) [SRC (listing)] | Switch 1 pipeline cache; not inspected further | none |
| Eden-NX2 | C++ | Same as Eden: `dynarmic` + NCE [SRC (listing)] | Ampere→Maxwell lowering into the inherited Maxwell IR (E-2) [SRC (listing)] | Inherited Eden pipeline cache; no `pipeline_cache_files` [SRC (listing)]; contents not inspected | none |
| Orchid | C++ | Same: `dynarmic` + NCE [SRC (listing)] | Ampere→Maxwell lowering; opt-in cooperative-matrix path for HMMA with a fallback substitute (E-2, E-9) [SRC (listing) + DOC] | Inherited from Eden-NX2; disk-loaded shaders re-run the tensor admission check (E-9) [DOC]; format not inspected | none |
| Anka | C++ | Same: `dynarmic` + NCE; Lichii bundled as a second core for one title [SRC (listing) + DOC] | Ampere→Maxwell lowering; SPIR-V pattern folding for a driver bug (E-2, E-8) [SRC (listing) + DOC] | Inherited from Eden-NX2; format not inspected | none |
| Hokkaido | C++ | Same: `dynarmic` + NCE [SRC (listing)] | Ampere→Maxwell lowering; whole-program rejection on failed global-memory tracking (E-2, E-3) [SRC (listing) + DOC-MEASURED] | Guest-environment records (not SPIR-V), per-record misses, version-suffixed files, cross-process lock, driver cache replaced by rename (E-5) [SRC (comments) + DOC-MEASURED]; startup loading strategy (full vs. bounded) not inspected | none |

How the C++ forks handle a shader not seen before (synchronous build vs. asynchronous build
with skipped draws) was not inspected; Q8 records it.

## 4. Findings

### 4.1 Independent projects

| # | Source | Observation | Level | Applies to Lince? |
|---|---|---|---|---|
| O1 | oboromi@c97664a:`core/Cargo.toml`, `core/src/cpu/unicorn_interface.rs`, `cpu_manager.rs` | The guest CPU is `unicorn-engine` 2.1 through its Rust crate, one instance per core. A `dynarmic` fork is declared as a submodule but not wired into the Rust core. The README says a new ARMv9 emulator (`rapid-arm-emu`) is being developed, but the core does not depend on it yet. | SRC | **Partially.** Unicorn is a C library, so ADR-0026 rules it out of release builds. It might work as a dev/CI-only differential oracle for the A64 interpreter, but its v8.1–v8.5 coverage (PAC, MTE, LSE2) is unverified (open question Q3). |
| O2 | oboromi@c97664a:`core/src/gpu/{sm86.rs,spirv.rs,mod.rs}` | SASS is translated **directly to SPIR-V**, one emitter method per opcode (~200 handlers, most still `todo!`/`unimplemented!`). Registers and predicates are emulated as memory, predication as select-masks. It uses a hand-written SPIR-V word emitter (no `rspirv`) and `ash` 0.38 with Vulkan 1.2 compute only. It has no IR, no structurization pass and no cache. `sm86.rs` was read only for its structure (dispatch shape and handler count), before L1 was identified; no opcode semantics, encodings or latencies from it appear in this note. | SRC | **No** for the design: ADR-0009 rejects direct SASS→SPIR-V (option 2). It confirms that a plain word emitter is enough for SPIR-V output. **Do not use the decoder** (see L1). |
| O3 | oboromi@c97664a:`core/src/nn/hipc.rs`, `nn/mod.rs`, `sys/mod.rs` | HLE is limited to a HIPC header parser and a service-dispatch skeleton with unit tests. | SRC | No; too early to compare. |
| O4 | rapid-arm-emu@d762465:`Cargo.toml`, `jit-compiler-core/` | An SSA IR (`ExecIr`) with block parameters is lowered by selectable **compile tiers**: Cranelift without optimization (Tier1), Cranelift optimized (Tier2), `libgccjit`, and LLVM (commented out). Cranelift is 0.133. | SRC | **Yes, as a data point** for ADR-0005. Another Rust project chose Cranelift. Using Cranelift at opt-level none as a baseline tier is the alternative to ADR-0005's `dynasm-rs`/`iced-x86` Tier B, and PoC data should compare the two. |
| O5 | rapid-arm-emu@d762465:`code-generator/`, `arm-instruction-decoder/`, `spec/` | The A64/A32/T32 decoders are **generated at development time** from Arm's machine-readable ISA XML (2026-03 release, kept in Git LFS) with `roxmltree`. The generated decoder crate has **zero dependencies** so it compiles first and never rebuilds on dependency changes. It ships with criterion benchmarks. | SRC | **Yes.** Generating from Arm's spec fits ADR-0004's "coverage tool" plan, and the zero-dependency decoder crate is a good build-time pattern. Licensing of the Arm XML must be settled first (L3, Q2). |
| O6 | rapid-arm-emu@d762465:`io-mmu/` | The soft MMU has page-granular mappings (anonymous, external host memory, demand-paged objects), an exclusive monitor, a TLB-style lookup cache, and "audited atomic primitives" for all guest accesses. Tests use `loom`. | SRC | **Yes, as a pattern** for ADR-0019 and the exclusive-monitor part of ADR-0004. `loom` is planned in T-021 (D-004). |
| O7 | rapid-arm-emu@d762465:`Cargo.toml` `[workspace.lints]` | Clippy `cast_possible_truncation`, `cast_sign_loss`, `cast_possible_wrap`, `cast_precision_loss` and `arithmetic_side_effects` are set to **deny** workspace-wide, on the rationale that "in an emulator correctness is everything". | SRC | **Partially.** This is a candidate for Lince's QUAL lint set in decoder and semantics crates; needs a maintainer decision. |
| O8 | rapid-arm-emu@d762465:`rapid-arm-emu/src/front_end/mod.rs` (1 line), `armv9/jit.rs` (52 lines) | No A64 instruction semantics are implemented yet. The last push was 2026-08-13. | SRC | **No:** no A64 semantics implemented yet. |
| P1 | pound@0bbb58e:`README.md`, `src/jit/jit_engine.c` | The CPU path combines Unicorn with Ballistic's engine and memory interfaces. The README task list includes "Translate SM86 to SPIR-V" (not done), `mimalloc` (done), and a JIT code cache allocator (not done). | SRC + DOC | **No:** C, Unicorn-based, no shader or cache design to compare yet. |
| P2 | pound@0bbb58e:`docs/SM86_TO_SPIRV_RULES.md` | Planned shader rules: structure-of-arrays IR; def-use chains built in the first pass; predicates mapped to SPIR-V booleans; predicated instructions expanded immediately; constant buffers as `Uniform` blocks; **UGPRs treated as ordinary per-thread registers, ignoring warp-level broadcast** "to increase translation speed". | DOC (plan, not implemented) | **Partially, as a warning.** Treating uniform registers per thread gives the same results only when the input to the uniform op is the same in every lane. What hardware does with a non-uniform input to `R2UR` is UNKNOWN. Lince's IR keeps uniform registers first-class (ADR-0009), and differential tests should cover non-uniform inputs (PoC-4). |
| P3 | ballistic@de07fca:`docs/IR_DESIGN_DOC.md` | The IR is **structured SSA**: `IF`/`LOOP` scopes with block arguments, `YIELD`/`MERGE` instead of phi nodes, and a fixed 64-bit instruction word (13-bit opcode, three 17-bit operand indices, one bit marking a constant-pool index). Allocation comes from one arena with no `malloc` during construction. Dead code becomes `NOP`, with compaction past a NOP ratio (example: 25%). Blocks are capped at 65,536 instructions. Loops are built by peeling one probe iteration. Tier 1 is a "dumb" translation, and Tier 2 starts when a block turns hot. | DOC | **Partially.** It is a well-specified design point for the CPU IR (ADR-0005) and also for shader structurization (ADR-0009 risk). The arena and no-allocation rule match Tier-0 pre-decode goals. Do not adopt the encoding without our own measurements. |
| P4 | ballistic@de07fca:`README.md`, `tools/scripts/generate_decoder_tables.lua`, `spec/arm64_xml/` | Decoder tables are generated (in Lua) from Arm's A64 XML, which is committed to the repository along with an Arm ARM PDF. The target is a very small cache footprint (L1d 4 KB, L2 30 KB). The roadmap lists treating the X30 pointer-authentication sign/authenticate instructions as NOP and then stripping the upper bits of X30. | SRC + DOC | **Partially.** It is a second project generating its decoder from Arm XML (see O5). The PAC-as-identity approach agrees with doc 01 E1 but is still unvalidated. Do **not** follow the practice of committing Arm XML or PDFs (L3). |

### 4.2 Eden and its Switch 2 forks

| # | Source | Observation | Level | Applies to Lince? |
|---|---|---|---|---|
| E-1 | eden@63e957b (path listing) | Eden `master` has **no Switch 2 code**: zero paths match `nx2`, `ampere`, `shader_cache_v2` or `pipeline_binar*`. Switch 2 work lives in forks. | SRC (listing) | Yes. Eden `master` is Switch 1 only. Switch 2 behavior knowledge comes from the forks, which doc 00 R3 ("no forks of yuzu/Ryujinx/Eden/Eglantine/Lichii") and LEGAL-20 (no code) cover equally. |
| E-2 | eden-nx2@50fa86b, Orchid@e9d4b5b, Anka@140d784, Hokkaido@32c4089 (path listings) | All four contain `src/shader_recompiler/frontend/ampere/lower_to_maxwell.h`, so the **Ampere→Maxwell lowering** is present from Eden-NX2 on. Orchid also has a `lower_to_maxwell_upstream.h`. None has a separate Ampere IR. | SRC (listing) | **Yes.** This confirms doc 01 E5 across the whole family. Adding instruction coverage there is ongoing work (Orchid README lists IMAD.WIDE, IADD3 carries, LEA.HI, F2I.U64 and others added for one title). ADR-0009 option 4 stands. |
| E-3 | Hokkaido@32c4089:`docs/bananza-global-atomics.md` | In one title, 24 of 32 "native QMD failed" compute rejections came from **global atomics with no storage buffer to route to**. The yuzu-lineage translator turns global memory accesses into storage-buffer bindings by tracking 64-bit pointers in constant buffers at compile time. When the tracked slot is unmapped at that moment, the access stays global, and a global atomic has no fallback, so the **whole program** is rejected. The fix is a narrower heuristic (pair the atomic with a pointer it was used with). A broader heuristic was tried and changed five unrelated modules. | DOC-MEASURED | **Key lesson for ADR-0009 / doc 03.** Turning global memory into storage buffers at compile time makes correctness depend on guest state. Lince should treat raw global addressing (buffer device address, physical storage buffer) as the general path and treat pointer tracking only as an optimization. A failed guess must never reject a program. |
| E-4 | Hokkaido@32c4089:`docs/bananza-global-atomics.md` "Verification" | Regressions are checked with a **shader sweep**: saved guest environments from real caches (2,239 dumps, 10,906 emitted modules for one title) are recompiled, every module goes through `spirv-val`, and the output must be byte-identical before and after unrelated changes. | DOC-MEASURED | **Yes** for ADR-0028 / PoC-4. Keep a corpus of recorded dependency snapshots (from private dumps, never committed per doc 09 §5) and require byte-identical IR/SPIR-V for no-op changes. The counts also size the problem: about 10⁴ modules per title. |
| E-5 | Hokkaido@32c4089:`src/video_core/pipeline_cache_files.h`, `renderer_vulkan/vk_pipeline_cache_records.h` (header comments) | The disk cache stores **guest environments (guest code plus the values translation read), not SPIR-V**, so emitter changes need no version bump. When translation starts asking for a value older records lack, those records fail **one by one** and their pipelines are rebuilt on use, rather than discarding the file. A list of older compatible versions lets a new build start from the newest compatible cache, copying it and never modifying it. File names carry the version because two builds sharing one name deleted each other's cache on every boot (logged as "Total Pipeline Count: 0", then hundreds of pipelines built on the GPU thread). The append-only environment file is guarded by a **file lock** against concurrent emulator instances, while the driver pipeline cache is replaced by rename. Duplicate records per key are pruned while the file stays append-only. | SRC (comments) + DOC-MEASURED | **Yes** for ADR-0015 / ADR-0016 / PoC-6. This supports Lince's L1 "dependency snapshot" layer as the durable artifact, with code generation re-derivable. Add: per-record "missing dependency → miss" (never invalidate the whole cache); version-suffixed file names; cross-process locking for writers. |
| E-6 | Anka@140d784:`NX2_GPU_THREAD_SPLIT.md` §"Light draws" | After the front/back split, the back thread was about 98% busy, and about 80% of it was draw setup spread over a dozen small steps. Most indirect draws changed only vertex-stream addresses and sizes, index ranges and draw parameters. A **light draw** path re-binds only vertex, index and indirect buffers when nothing else changed since the last fully set-up draw and nothing was recorded, invalidated or flushed in between. Measured on one scene: 100,563 of 187,178 draws per second went light, and 23.0 → 29.7 FPS with no visible difference. A first attempt had 0% hits because size registers written together with addresses counted as "state". | DOC-MEASURED (one machine, one title) | **Yes** for ADR-0008 / PoC-9. Track a per-draw "changed since last recorded draw" set on the front end and use a minimal-rebind path when it is limited to stream and draw parameters. Classify registers carefully. Lince's frame ledger should count why draws missed the fast path, as Anka's profile does. |
| E-7 | Anka@140d784:`README_ANKA.md` "Measured results" | Running terrain indirect draws on the host GPU gave about 20.8 → 30.8 FPS (camera not identical). Splitting the back thread again (register work ahead of the cache/recording stage) gave +7–9%. PGO gave +15–22%. Reactive flushing helped in one scene (+13–23%) and hurt in another (−23–30%). The author marks these as single-machine, partly uncontrolled A/B runs. | DOC-MEASURED | **Partially.** Native indirect draws and further pipelining are directional for ADR-0008. PGO is a release-build question for ADR-0002. The reactive-flushing swing shows that guest-visible readback policy is scene-dependent (ADR-0022). |
| E-8 | Anka@140d784:`README_ANKA.md` "Known issues" | On NVIDIA drivers 617 and newer, a `max(x, -x)` pattern in a title's vertex shaders triggers `VK_ERROR_DEVICE_LOST`. Anka folds it before handing SPIR-V to the driver. | DOC | **Yes** for ADR-0007: a candidate entry for Lince's driver-workaround list. It must be reproduced with Lince's own SPIR-V before adoption (UNVERIFIED). |
| E-9 | Orchid@e9d4b5b:`docs/experimental-cooperative-matrix.md` | Tensor-core `HMMA.16816.F16` is lowered to `VK_NV_cooperative_matrix` only when the device advertises subgroup-scope M16 N8 K16 FP16 in compute, subgroup size control can force 32 lanes with full subgroups, `shaderFloat16` survives driver workarounds, and SPIR-V ≥ 1.4 is available. The path is opt-in. The admission check is repeated for disk-loaded shaders (collectives before any structured control flow; 1D workgroups of 32–1024 in steps of 32). Scratch is 1,280 bytes per subgroup. A known NVIDIA Ampere FP16 workaround makes an RTX 3080 ineligible, and KHR-only devices are ineligible. "No currently tested device/driver is numerically qualified." Rejection falls back to a non-tensor upscale substitute, cached by compute key. | DOC | **Yes** for ADR-0007 tiers and T-016. This is a concrete admission checklist for a tensor-op tier. It confirms that the cooperative-matrix route is not yet numerically validated anywhere. Lince needs its own numeric qualification against reference HMMA results (T-023 hardware). |
| E-10 | Hokkaido@32c4089:`docs/FE_UPSCALE_STANDIN.md`, `README.md` | Once HMMA support let one title run its own upscaling network, a trailing artifact appeared. The workaround replaces the final dispatch with a spatial upscale, gated on title id and a **program fingerprint**. One title's movie-enhancement network is skipped entirely on Android. | DOC | **Yes, as a counter-example.** Substitution keyed to a title and fingerprint is what doc 00 R7 (see doc 01 E7) forbids as a silent default. Any approximation must be labelled and opt-in. |
| E-11 | Orchid@e9d4b5b:`docs/host-stall-diagnostics.md` | Opt-in stall sampler: fixed host-call "phase" records (capture, lowering, pipeline prepare, the actual `vkCreate*Pipelines` call, pipeline-build waits, queue drain vs. lock acquisition, submit lock vs. submit call, fence and semaphore waits) are sampled once a second for a bounded window and never take GPU or cache locks. The document states that it is not timing-neutral. | DOC | **Yes** for ADR-0027 / PoC-7. It is a useful list of phases the frame ledger must distinguish, especially "waiting for a lock" vs. "inside the driver call". |
| E-12 | Orchid@e9d4b5b:`docs/performance/TargetGames.md` §"Acceptance and evidence" | Measurement protocol: record build hash, binary SHA-256, game and update, host hardware and driver, and immutable save/config/cache seed; at least three baseline and three candidate runs in alternating order; report median/min/max, p50/p95/p99 and counts above 16.67/33.33/50/100 ms; frame generation does not count toward the target; keep diagnostic traces out of timed runs. | DOC | **Yes** for PoC-10 and the "performance claims need data" rule. Lince can adopt the same reporting fields. |
| E-13 | Orchid@e9d4b5b:`NX2_NOTES.md` rows 123–127; Hokkaido@32c4089:`NX2_NOTES.md` rows 123–127 (same content in both) | Newer updates of one title block at boot unless a GameChat service and a camera-context service answer. Stub sessions and events are enough to proceed. | DOC | **Partially** for T-019 / ADR-0024. Some system services must exist as stubs even for titles that never use them. Their names in these notes come from symbols in game binaries; Lince must not adopt them (L2). |
| E-14 | Orchid@e9d4b5b, Anka@140d784, Hokkaido@32c4089 `NX2_NOTES.md` / `README*` | All three are keyed to specific titles: per-game defaults, per-game presets, and per-title behavior toggles (dozens of environment switches). | DOC | **No** as a design. Lince's rule is correct-by-default behavior, with per-title data only as cache content. |

### 4.3 Anka Native (separate category)

| # | Source | Observation | Level | Applies to Lince? |
|---|---|---|---|---|
| N1 | AnkaNative@09bf66e:`README.md`, `NOTICE.md` | Not an emulator. Each supported title's ARM64 code is **rewritten** and runs natively on Android, a small host answers system calls, and NVN graphics are replayed on Vulkan. Shaders are translated **ahead of time per title** by Ryujinx's (MIT) translator, plus a GPL Switch 2 mesh-shader lowering from Eglantine. One title has about 42,000 shaders; preparing it on a phone took over an hour. Per-title profiles hold code offsets and patch bytes. | DOC + DOC-MEASURED | **Partially.** It is a data point for shader counts (about 4×10⁴ per title) and for ADR-0005 option 4 (native execution on ARM64 hosts). The per-title code-patching approach is out of scope for Lince. |

## 5. Legal flags (stop-and-report items)

| # | Item | Why it matters | Action taken / recommended |
|---|---|---|---|
| L1 | oboromi@c97664a: `scripts/sm_86_instructions.txt` (≈120k lines, added in `8b7e67939f26` "[gpu] sm86 decoder scripts"), `scripts/sm_86_latencies.txt`, `docs/sm86.pdf` (7.8 MB), and the decoder generated from them (`core/src/gpu/sm86_decoder_generated.rs`, `sm86.rs`) | The text file's header and structure (architecture block, relocator table, per-pipe operation sets) look like a **machine-description table from NVIDIA's toolchain**. The repository gives no provenance. If it was extracted from vendor binaries, using it would conflict with LEGAL-10/LEGAL-14. The PDF was not opened. | Reading stopped after the first lines identified the format. **Recommendation:** treat oboromi's SASS decoder, its tables and anything generated from them as off-limits for Lince (T-018 must not consult them), and the maintainer decides in [D-006](../decisions/D-006-oboromi-sass-tables-off-limits.md). `sm86.rs` was read for structure only (see O2). |
| L2 | Eden-lineage `NX2_NOTES.md` (all forks) | Service, function and structure names in the notes come from **symbols in game binaries** (the notes describe mapping module offsets to exported names). They also cite code offsets inside game modules. | Lince must not adopt these names (LEGAL-12) or cite the offsets. Behavior descriptions only. |
| L3 | Ballistic (`spec/arm64_xml/notice.xml`, Arm ARM PDF) and rapid-arm-emu (`spec/*.tar.gz`, Arm PDFs) | Both commit Arm's specification files. The XML `notice.xml` reserves reproduction rights to Arm. | Lince may **read** Arm's public specification (LEGAL-11). Committing it, or tables generated from it, needs a license check first (Q2). |
| L4 | Anka, Hokkaido, Anka Native READMEs | They tell users to install their own keys and firmware (Anka, Hokkaido), or run patched game code (Anka Native). | Nothing about key or firmware handling was read or recorded (ADR-0029, LEGAL-05). |

## 6. Rust crates worth evaluating

| Crate | Seen in | Use for Lince | Status in Lince |
|---|---|---|---|
| `cranelift` (0.133, `jit`/`frontend`/`native`/`module`) | rapid-arm-emu O4 | Tier C, and possibly Tier B at opt-level none | Proposed in ADR-0005 / doc 04. Add "Cranelift opt-none as Tier B" as a measured alternative. |
| `roxmltree` | rapid-arm-emu O5 | Development-time decoder generator from Arm XML | Candidate for an `xtask` generator, only after Q2. |
| `loom` | rapid-arm-emu O6 | Concurrency model tests | Already planned (T-021). |
| `ash` | oboromi O2 | Vulkan bindings | Chosen in doc 04 §Vulkan bindings row; planned for PoC-5 (T-012). |
| `criterion` | rapid-arm-emu O5 | Decoder microbenchmarks | Candidate for `cargo xtask bench`. |
| `unicorn-engine` | oboromi O1 | Possible dev/CI-only A64 differential oracle | C library: ADR-0026 "dev/CI only" category; coverage unknown (Q3). |
| `gccjit` | rapid-arm-emu O4 | — | Not recommended (C dependency, GCC runtime). |

## 7. Open questions

1. **Q1 (maintainer, legal):** Declare oboromi's SASS tables and generated decoder off-limits
   (L1)? Recorded as [D-006](../decisions/D-006-oboromi-sass-tables-off-limits.md).
2. **Q2 (legal):** Under what license does Arm publish the A-profile machine-readable
   specification today? RECALL, unverified: newer releases of Arm's MRS were published under a
   permissive license, while the older XML carries the restrictive notice seen in L3. That
   decides whether Lince can commit a generated A64 decoder.
3. **Q3:** Does Unicorn 2.1's AArch64 model cover the v8.1–v8.5 features Switch 2 titles use
   (doc 01 E1)? If not, it cannot serve as an oracle for them.
4. **Q4:** Is an Eden pull request merging Switch 2 support pending? The Forgejo pulls API did
   not return JSON from the research environment, so this is unchecked.
5. **Q5:** Does Lichii still use its `shader_cache_v2` log (doc 01 E17), given that its fork
   Hokkaido has a different, environment-record cache? This needs a re-pin of Lichii.
6. **Q6:** Does the NVIDIA `max(x, -x)` device-lost bug (E-8) reproduce with Lince-generated
   SPIR-V, and on which driver versions?
7. **Q7:** What do hardware uniform-datapath instructions do with lane-varying inputs (P2)?
   This needs a CUDA kernel in the T-014 corpus and the T-023 hardware run; CUDA source may
   never make the compiler emit `R2UR` with a non-uniform input, so the corpus may not be able
   to answer it. Listed as UNKNOWN in doc 08 §4.
8. **Q8:** How do the Eden-lineage forks handle an unseen shader at draw time (block, or build
   asynchronously and skip the draw), and does Hokkaido's cache load all records at startup or
   on demand? Not inspected; relevant to invariants 1 and 4 in AGENTS.md.

## 8. Affected ADRs, PoCs and backlog tasks

- **ADR-0004** (guest CPU): generate the A64 decoder and coverage tool from Arm's spec (O5, P4),
  subject to Q2.
- **ADR-0005** (JIT tiers): add "Cranelift opt-none as Tier B" as a measured alternative (O4).
  Ballistic's structured-SSA IR is a reference design (P3).
- **ADR-0007** (host graphics tiers): cooperative-matrix admission checklist (E-9). Driver
  workaround candidate (E-8).
- **ADR-0008** (GPU command architecture) / **PoC-9**: light-draw fast path and draw-delta
  tracking (E-6). Native indirect draws and further back-end staging (E-7).
- **ADR-0009** (shader IR) / **PoC-4**: global memory through raw addressing, not compile-time
  pointer tracking (E-3). Uniform-register semantics tests (P2). Lowering remains rejected (E-2).
- **ADR-0015 / ADR-0016** (cache format and indexing) / **PoC-6**: store dependency snapshots as
  the durable record; per-record misses; version-suffixed files; cross-process writer lock (E-5).
- **ADR-0022** (synchronization): readback policy is scene-dependent (E-7).
- **ADR-0024** / **T-019**: stub-only services needed at boot by newer title updates (E-13).
- **ADR-0026**: Unicorn as a dev/CI-only oracle, if Q3 is positive.
- **ADR-0027** / **PoC-7**: stall phase taxonomy (E-11).
- **ADR-0028** / **PoC-4**: shader-sweep regression corpus (E-4).
- **ADR-0028** / **PoC-10**: A/B measurement protocol (E-12).
- **QUAL rules**: consider denying lossy-cast and unchecked-arithmetic Clippy lints in decoder
  and semantics crates (O7).
- **T-016**: include `VK_NV_cooperative_matrix`, `VK_KHR_cooperative_matrix` and subgroup size
  control (full subgroups) per vendor (E-9).
- **T-018**: blocked on [D-006](../decisions/D-006-oboromi-sass-tables-off-limits.md); must not
  consult oboromi's SASS decoder or tables (L1).
- **Doc 01 §8, doc 00 §7 and doc 08 §4**: updated in this task to point here (doc 08 gains the
  `R2UR` lane-varying-input UNKNOWN from P2/Q7).

## 9. Sources

- oboromi — https://github.com/0xNikilite/oboromi/tree/c97664a8460e11bf86800eef87ae17446c005d40
- rapid-arm-emu — https://github.com/0xNikilite/rapid-arm-emu/tree/d762465dfd0579acba81a8dc944296522a61f929
- Pound — https://github.com/pound-emu/pound/tree/0bbb58e85a326665a3302e45fbfbd02206ded67c
- Ballistic — https://github.com/pound-emu/ballistic/tree/de07fcaa7708a06380810e808051faca8b53cf25
- Eden — https://git.eden-emu.dev/eden-emu/eden/src/commit/63e957b1cad697558beb782b0947f01227efc826
- Eden-NX2 — https://git.eden-emu.dev/Takoroka/eden-nx2/src/commit/50fa86bbbc403ad565eb44fcc281f16f77c69e15
- Orchid — https://github.com/MythosOfs/Orchid/tree/e9d4b5b795e585924b4dca164fe8122d45b81929
- Anka — https://github.com/EddizEge/Anka/tree/140d784a7e4d12a8361233105a0fd08fa0ab2bfa
- Anka Native — https://github.com/EddizEge/AnkaNative/tree/09bf66ea6200f92e03f89ce0297ba78857b29a00
- Hokkaido — https://github.com/pavelgajdo/Hokkaido/tree/32c4089558e25395aa026c594f2b70e5773360b1
