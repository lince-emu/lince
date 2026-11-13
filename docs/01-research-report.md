# 01 — Research Report: How Existing Emulators Handle Shaders, Pipelines and Caches

Status: First pass · Research date: 2026-11-13

## 0. Method and evidence levels

Sources were inspected directly at the commits below. Each finding carries an evidence level:

- **[SRC]** — read in source code at the cited commit.
- **[DOC]** — stated in the project's own technical documentation at the cited commit.
- **[DOC-MEASURED]** — the project's documentation reports a measurement; Lince has not
  reproduced it.
- **[MIRROR]** — read from a third-party mirror of an older version; may be outdated.
- **[RECALL]** — from prior knowledge; not re-verified in this pass. Treat as a hypothesis.

| Repository | Branch | Commit inspected | Date | Language / license |
|------------|--------|------------------|------|--------------------|
| github.com/Eglantine-Community/Eglantine | `nx2` | `66ce46feb67a` | 2026-11-12 | C++ / GPL (Eden/yuzu lineage) |
| github.com/zortos293/Lichii | `nx2` | `0da698bb98c6` | 2026-11-13 | C++ / GPL (Eglantine/Eden/yuzu lineage) |
| github.com/dolphin-emu/dolphin | `master` | `e36cd32` | 2026-11-12 | C++ / GPL-2.0+ |
| github.com/cemu-project/Cemu | `main` | `9cac875` | 2026-11-12 | C++ / MPL-2.0 |
| Ryujinx (mirror: git.dawidpotocki.com/mirror/ryujinx) | tags 1.1.x | various | ≤ 2024 | C# / MIT |

**Clean-room note.** All of the above are copyleft or differently licensed codebases. Lince
records *observations about behavior and design*, never code. See
[`09-legal-and-content.md`](09-legal-and-content.md).

---

## 1. Eglantine and Lichii (Switch 2, yuzu/Eden lineage)

Both projects are C++ forks of Eden (itself from yuzu/Sudachi) extended to boot Switch 2 titles.
Lichii is a fork of Eglantine; the two share `NX2_NOTES.md` and `NX2_GPU_THREAD_SPLIT.md`
(identical line counts at the commits above).

### 1.1 Findings

| # | File | Observation | Level | Applies to Lince? |
|---|------|-------------|-------|----------------------|
| E1 | `NX2_NOTES.md` rows 3, Titles table | Switch 2 executables use ARMv8.1–8.5 instructions heavily: LSE atomics, CAS/CASP, LDAPR, RETAA/RETAB and other PAC instructions, MTE `STG`. Counts in the hundreds of thousands per title. Stock dynarmic did not decode them. | DOC-MEASURED | **Yes.** The A64 decoder must cover v8.1–v8.5 from day one; PAC can be identity, MTE tags zero (their approach) — to be validated. |
| E2 | `NX2_NOTES.md` Titles | All tested titles: 64-bit, 39-bit address space, core mask `0x3f` (6 cores), new SVC `0x80`. | DOC | **Yes.** Address-space and core model. |
| E3 | rows 77–79 | Folding the guest's 6 cores onto 4 emulated cores changed which heap a job allocated from and caused crashes. | DOC-MEASURED | **Yes.** Emulate the real 6-core layout; never fold cores. |
| E4 | rows 31, 53 | NVN2 binds Ampere classes (`C797` 3D, `C7C0` compute, `C7B5` DMA) and drives state through **Turing MME64 macros**; compute uses **QMD 3.0**. | DOC | **Yes.** Needs a native MME64 implementation. |
| E5 | row 54; `.claude/skills/nx2-shader-triage/SKILL.md` | Shaders are Ampere SASS (they label it SM86). They are **lowered to Maxwell instructions** and fed to yuzu's Maxwell recompiler. "Any instruction it refuses rejects the whole program, and every draw or dispatch using it is skipped." | SRC + DOC | **Key lesson.** Lowering to an older ISA creates whole-program rejection and draw skipping. Lince decodes Ampere natively into its own IR (ADR-0009). |
| E6 | rows 88, 92, 93, 95 | Many semantic traps: SM70+ `MUFU.SIN/COS` take turns, not radians; predicate save/restore bug in uniform guards caused color casts; jump tables past the first `EXIT`. | DOC-MEASURED | **Yes.** Differential testing of every opcode is mandatory (PoC-4). |
| E7 | row 64; triage skill | A tensor-core (HMMA) upscaling network is replaced by a **bilinear blit** unless an opt-in emulation is enabled; full emulation took the host driver minutes to compile and would cost seconds per frame. | DOC-MEASURED | **Yes — as a warning.** This is substitution of rendering work. Lince must offer a correct path and label any approximation. |
| E8 | row 89 | Task/mesh shaders exist in shipped titles and map to `VK_EXT_mesh_shader`. Those pipelines were kept out of the disk cache. | DOC | **Yes.** Mesh support affects Tier 0 coverage (doc 03). |
| E9 | row 121 | A 28,102-instruction compute shader took 3.4 s to translate on the GPU thread (0.85 s after optimization). Translation was not moved to workers because "environments read live GPU state (texture types, constant buffer values)". | DOC-MEASURED | **Key lesson.** Translation must consume an explicit dependency snapshot so it can run off-thread (ADR-0009). |
| E10 | `NX2_GPU_THREAD_SPLIT.md` | In Mario Kart World the single GPU thread was ~100% busy while the host GPU was 20–40% busy. Per second: command processing ~335 ms, macros ~200 ms (1.3 M calls), draw setup ~345 ms (55,000 draws). A front/back split gave 1.46× throughput, frames 17% shorter. | DOC-MEASURED | **Key lesson.** Shader work is not the only stutter source. Design the GPU front-end as a pipeline from day one (ADR-0008). |
| E11 | rows 117, 140 | ~1 M macro calls/s, 96% exact repeats; memoization replays 97%; an x86-64 MME JIT followed. | DOC-MEASURED | **Yes.** MME64 JIT + memoization candidates. |
| E12 | rows 99–107, 114, 139 | VRAM blow-up from ASTC→RGBA8 (8192×8192×68 virtual textures ≈ 23 GB decoded), texture churn, CPU transcode threads saturating cores; fixed with GPU ASTC decode + BC re-encode, sparse unbacked images, larger allocation blocks. | DOC-MEASURED | **Yes.** Texture pipeline requirements (ADR-0020). |
| E13 | rows 120, 123–124, 131 | Lock contention on cache mutexes, 5,500–7,000 fences/s, 250–330 MB/s of buffer read-back at fences. | DOC-MEASURED | **Yes.** Synchronization design (ADR-0022). |
| E14 | `docs/nx2/async-native-pipelines.md` (Lichii) | On a pipeline miss, environment reads and IR translation stay **synchronous on the GPU back thread**; SPIR-V emission, shader-module creation and driver compile go to workers. The draw is held (not discarded) and later commands wait behind it: "this is overlap, not free work." | DOC | **Yes.** Confirms that async compile without a fallback still stalls. |
| E15 | same doc; `renderer_vulkan/async_draw_policy.h` | Legacy yuzu async-shader mode **discarded draws** by heuristic (depth enabled or > 6 indices). Lichii refuses to discard any native (Switch 2) draw. | SRC + DOC | **Yes.** Confirms R7. |
| E16 | same doc | Eglantine PR #503 (at `7584d618`) did full async translation with detached environments, but its timeout discards draws and its texture snapshots read live descriptor memory without synchronization. | DOC (third-party description) | Lesson: snapshots must be complete and immutable. |
| E17 | `shader_cache_v2/cache_format.h` | "Shader cache v2": one **append-only log** per title. 32-byte header; records 8-byte aligned with magic, kind, key, CRC-32 header, payload, XXH3-64 payload hash. Latest record per (kind,key) wins; **tombstones**; unknown kinds preserved; torn tails ignored. | SRC | **Yes — good ideas** (framing, checksums, tombstones, torn-tail handling). |
| E18 | `shader_cache_v2/cache_file.h`, `vk_pipeline_cache.cpp` `LoadDiskResources` | The **whole log is read into memory at boot** and parsed (in parallel); "used" pipelines from previous sessions are rebuilt at boot ("Loading Shaders" waits for that batch); background prebuild follows. A `.booting` marker disables boot builds after a crash during loading. | SRC | **Counter-example for R8.** Boot cost still grows with the log (bounded by compaction). Lince uses an index and lazy resolution instead. |
| E19 | `shader_cache_v2/driver_blobs.h`, `renderer_vulkan/vk_pipeline_binaries.*` | Driver layer uses `VK_KHR_pipeline_binary` in a separate log per device/driver identity; mismatched identity discards the file; size bounds 512/384 MiB. | SRC | **Yes.** Per-pipeline driver binaries enable lazy loading (ADR-0013). |
| E20 | `shader_jobs.h` | Job priorities `NeedNow` / `Soon` / `Background`; a Background job queued long enough competes with Soon. | SRC | Partially. Lince needs priority by *cost to the frame*, plus a frame-aware governor (ADR-0018). |
| E21 | `NX2_NOTES.md` row 55, 59 | The guest ran a full ring of command memory ahead of the GPU thread when presents were not paced; a bounded wait for the frame fence fixed it (250 ms → 4 s timeout because shader compiles exceeded 250 ms). | DOC-MEASURED | **Yes.** Shows how compile stalls propagate into guest pacing. |

### 1.2 What is cached, when, and what blocks (Lichii `nx2` @ `0da698b`)

| Question | Answer |
|----------|--------|
| What is cached | Guest program records (lowered code + environment), pipeline records, used-pipeline lists, driver binaries (`VK_KHR_pipeline_binary`) or a `VkPipelineCache` blob. |
| When compilation occurs | At boot (previously used pipelines), in background prebuild, and on demand at draw time. |
| Asynchronous stages | SPIR-V emission, shader-module creation, driver compile (native graphics, when a reservation is available). |
| Synchronous stages | Environment reads + IR translation on GPU back thread; emission when the budget is full. |
| Cache miss | Draw is recorded as pending; the Vulkan worker holds it until the pipeline is ready. |
| Gameplay waits for pipeline | **Yes** (held, not discarded) for Switch 2 titles. |
| Startup affected by cache | **Yes**: full log read + boot builds behind a loading screen. |
| Immediate fallback | **No** generic execution path. |
| Background work can spike | Yes; the doc notes queue delays and draw holds. |

---

## 2. Dolphin (GameCube/Wii)

| # | File | Observation | Level |
|---|------|-------------|-------|
| D1 | `Source/Core/VideoCommon/VideoConfig.h:43-49` | `ShaderCompilationMode { Synchronous, SynchronousUberShaders, AsynchronousUberShaders, AsynchronousSkipRendering }`. | SRC |
| D2 | `ShaderCache.cpp:74,100` | `bWaitForShadersBeforeStarting` optionally compiles cached shaders before the game starts. | SRC |
| D3 | — | Hybrid mode: draw with an ubershader immediately; compile the specialized shader in the background; switch when ready. Works because GameCube/Wii pixel and vertex pipelines are **configurable fixed function (TEV)** with a bounded configuration space that a single interpreting shader can cover. | RECALL (well-documented in Dolphin's 2017 "Ubershaders" article) |

**Why it works:** the ubershader interprets *configuration*, not arbitrary code. Its inputs are
a few hundred bytes of register state per draw, and the full space of TEV behavior is known.

**Limits that carry over:** ubershaders are large and slow to compile once; on some drivers
the ubershader itself caused a one-time hitch; runtime cost is higher than specialized shaders;
switching shader variants per draw costs driver work.

**What does not carry over:** Switch 2 shaders are arbitrary SASS programs. An equivalent is an
interpreter of *code*, which is a different and harder problem (doc 03 §3).

---

## 3. Cemu (Wii U)

| # | File | Observation | Level |
|---|------|-------------|-------|
| C1 | `Renderer/Vulkan/VulkanRendererCore.cpp:194-217` | `IsAsyncPipelineAllowed` is a heuristic: draws to a framebuffer with a depth buffer may compile asynchronously; ≤ 6 indices are considered "essential"; one 1600×1600 resolution is hard-coded as essential for a specific game. | SRC |
| C2 | same file `:250-271` | With `async_compile` on and the draw judged non-essential, a cache-only creation is tried; on failure, compilation is queued and the **draw sequence is skipped** (`drawSequenceSkip`). Otherwise compilation is synchronous. | SRC |
| C3 | `VulkanPipelineStableCache.cpp` | Persistent pipeline cache compiled behind a loading screen at boot. | SRC (structure), RECALL (UI behavior) |

**Lesson:** heuristic "essential draw" classification is per-game folklore and violates R7.

---

## 4. yuzu (Switch 1) — via its descendants

yuzu's repository is offline. Its Vulkan pipeline cache survives (modified) in Eden, Eglantine
and Lichii.

| # | Observation | Level |
|---|-------------|-------|
| Y1 | Disk cache stores guest shader **environments** (code + the GPU state each translation read), so programs can be retranslated after emulator updates. `LoadDiskResources` rebuilds all cached pipelines behind a "Loading shaders" progress screen at boot. | SRC (in descendants, `LoadDiskResources`, `MigrateLegacyPipelineCache`) + RECALL |
| Y2 | "Asynchronous shaders" option skips draws while their pipeline compiles (heuristic, see E15). | SRC (descendants) |
| Y3 | Shader translation reads live GPU state through an `Environment` interface: `ReadInstruction`, `ReadCbufValue`, `ReadTextureType`, `ReadTexturePixelFormat`, `ReadViewportTransformState` (`src/shader_recompiler/environment.h:77-114` in Lichii). | SRC |
| Y4 | The environment idea is sound: a shader variant is identified by code **plus** the state it depends on. Lince keeps the idea but makes the dependency set explicit and immutable (ADR-0009). | Analysis |

---

## 5. Ryujinx (Switch 1)

The current repository was not reachable. Findings come from a mirror of 1.1.x tags.

| # | Observation | Level |
|---|-------------|-------|
| R1 | `ShaderCache.cs` uses a `ParallelDiskCacheLoader` at startup over guest and host caches; host binaries are rebuilt from guest code when rejected by the driver. | MIRROR |
| R2 | `BackgroundDiskCacheWriter` writes new programs to disk on a worker queue. | MIRROR |
| R3 | The CPU uses PPTC (Profiled Persistent Translation Cache): translated guest code persisted and reloaded on later boots; background translation of new code. | MIRROR + RECALL |
| R4 | Guest translation (Maxwell → IR → GLSL/SPIR-V) is distinct from host driver compilation; on Vulkan the latter remains a major stall source. | RECALL |

**Lesson:** separating guest-derived artifacts (portable) from host-derived artifacts
(driver-specific) is right and is kept in Lince's layered cache.

---

## 6. Comparative summary

| | Lichii/Eglantine | Dolphin (hybrid) | Cemu | yuzu | Ryujinx |
|---|---|---|---|---|---|
| Cached | Guest program + env, pipelines, driver binaries | Specialized shaders, ubershaders, driver cache | Shaders, pipelines | Guest env + pipelines + driver cache | Guest + host shaders, CPU PPTC |
| Boot cost grows with cache | Yes (full log read, boot builds) | Optional wait | Yes | Yes | Yes |
| Immediate generic fallback | No | **Yes** (ubershader) | No | No | No |
| On miss | Hold draw | Draw with ubershader | Skip or stall | Stall or skip | Stall (background translation) |
| Draw discard | Refused for Switch 2 | No (hybrid mode) | Yes (heuristic) | Yes (option) | — |
| Remaining stutter sources | Translation on GPU thread, driver compile, GPU-thread saturation | Ubershader compile, driver | Driver compile | Driver compile | Driver compile |

**Architectural gap no project fills for a programmable GPU:** an immediate, correct execution
path for arbitrary unseen shader *code*. That gap is the reason Lince exists.

---

## 7. Modern Vulkan capabilities relevant to compile stalls

| Capability | What it gives | Caveat |
|------------|---------------|--------|
| `VK_EXT_graphics_pipeline_library` | Compile vertex-input, pre-rasterization, fragment and fragment-output parts separately; fast link without LTO; optional optimized re-link. | Fragment shader part still needs a driver compile for new code. Driver quality varies. |
| `VK_EXT_shader_object` | Per-stage shader objects; most state dynamic; no pipeline objects. | Availability and performance vary by vendor and driver. |
| `VK_KHR_pipeline_binary` (Vulkan 1.3.294, Aug 2024) | Application-managed per-pipeline binaries with keys; enables lazy, per-entry loading. | Not universal; Lichii restricts it to some drivers. |
| `VK_EXT_pipeline_creation_cache_control` (core 1.3) | `FAIL_ON_PIPELINE_COMPILE_REQUIRED` — try to create a pipeline from cache only, never compile on that call. | Hit rate depends on the driver's internal cache. |
| Extended dynamic state 1/2/3, vertex input dynamic state | Fewer pipeline permutations. | EDS3 coverage varies widely. |
| `VK_EXT_mesh_shader` | Task/mesh stages (needed by Switch 2 titles, E8). | Not on all GPUs. |
| `VK_KHR_cooperative_matrix` | Tensor-core-like matrix ops. | Vendor-limited; semantic match to HMMA must be verified. |

---

## 8. Not yet inspected (next pass)

Done in T-015: Eden (upstream), Orchid, oboromi, Pound, Hokkaido and Anka are covered in
`docs/research/switch2-emulator-projects.md` (oboromi is confirmed Rust there).

## 9. Sources

- [Eglantine `nx2`](https://github.com/Eglantine-Community/Eglantine/tree/nx2) — `NX2_NOTES.md`, `NX2_GPU_THREAD_SPLIT.md`, `.claude/skills/nx2-shader-triage/`
- [Lichii `nx2`](https://github.com/zortos293/Lichii/tree/nx2) — `docs/nx2/async-native-pipelines.md`, `src/video_core/shader_cache_v2/`, `src/video_core/renderer_vulkan/vk_pipeline_cache.cpp`, `src/video_core/renderer_vulkan/async_draw_policy.h`
- [Dolphin](https://github.com/dolphin-emu/dolphin) — `Source/Core/VideoCommon/VideoConfig.h`, `ShaderCache.cpp`
- [Cemu](https://github.com/cemu-project/Cemu) — `src/Cafe/HW/Latte/Renderer/Vulkan/VulkanRendererCore.cpp`
- [Ryujinx mirror, ShaderCache.cs 1.1.730](https://git.dawidpotocki.com/mirror/ryujinx/plain/Ryujinx.Graphics.Gpu/Shader/ShaderCache.cs?h=1.1.730)
- [VK_KHR_pipeline_binary](https://docs.vulkan.org/refpages/latest/refpages/source/VK_KHR_pipeline_binary.html) · [VK_EXT_graphics_pipeline_library](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_graphics_pipeline_library.html) · [VK_EXT_shader_object](https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_shader_object.html)
