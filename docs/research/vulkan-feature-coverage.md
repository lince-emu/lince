# Vulkan feature coverage per vendor and driver

- **Status:** First pass (T-016). Input for ADR-0007 (capability tiers) and ADR-0013 (pipeline
  creation). Not binding.
- **Data collected:** 2026-11-13. Every row below carries the submission date of the report it
  comes from; coverage percentages carry the database's "last updated" timestamp.
- **Source:** public data only — the Vulkan Hardware Database (`vulkan.gpuinfo.org`), the Vulkan
  specification (Khronos), Mesa release notes (`docs.mesa3d.org`), the NVIDIA Vulkan beta driver
  page, the AMDVLK GitHub releases, the MoltenVK `Whats_New.md`, and a Khronos blog post. Each
  claim cites its URL below. Access date for every URL: 2026-11-13.
- **Attribution:** device data from the Vulkan Hardware Database © 2016-2026 by Sascha Willems,
  https://vulkan.gpuinfo.org, licensed under the Creative Commons Attribution 4.0 International
  License (https://creativecommons.org/licenses/by/4.0/). Tables here are excerpts and summaries of
  the database; values were transcribed by Lince.
- **Evidence level:** DOC-MEASURED for per-device capability reports and coverage percentages
  (third parties ran the driver and published the result; Lince has not reproduced them on that
  hardware); DOC for the specification, release notes and vendor pages. Where nothing public was
  found the cell says UNKNOWN.
- **Applies to Lince:** yes — it decides which ADR-0007 tier each host lands in and where the
  per-driver override table (ADR-0007 "Risks", ADR-0013) needs entries. It is capability data
  only: a feature being advertised says nothing about its speed or correctness (see §5).

## 1. Question

Which drivers expose the features ADR-0007's tiers depend on — graphics pipeline libraries with
fast linking, shader objects, pipeline binaries, fragment barycentrics, subgroup size control
(and specifically: can the subgroup size be forced to 32?) and mesh/task shaders — and how does
that map onto tiers V1–V3?

## 2. Short answer

- On current desktop drivers for Windows and Linux, **NVIDIA proprietary, AMD (proprietary and
  RADV) and Intel ANV expose the five V3 features surveyed** (GPL with fast link or shader
  objects, pipeline binary, barycentrics, subgroup size control) **plus mesh/task shaders**, which
  ADR-0007 keeps as a separate flag. NVK exposes the same set in a Mesa 26.3 development build
  (R4); its status in released Mesa is only partly known (§3). GPL fast linking is reported `true`
  on every sampled driver that exposes GPL.
- **Intel's Windows driver** is the outlier among desktop targets: GPL + fast link, but **no
  shader objects and no pipeline binary** (Arc B580, driver 101.9175, 2026-11-05). By ADR-0007
  as written it is **V2**, not V3.
- **Subgroup size 32:** NVIDIA is always 32. AMD supports 32–64; current drivers (R5, R7, R8) can
  *require* 32 only in fragment, compute, task and mesh stages, and the older proprietary driver
  in R6 only in compute. Intel supports 16/8–32 and can require it only in compute, task and mesh
  (compute only on Xe-LP). **Qualcomm (proprietary and Turnip) cannot run 32 at all**
  (minimum 64).
- Mobile (Adreno) and Apple (MoltenVK, KosmicKrisp) land in V2 or below; neither is a first-wave
  platform under ADR-0003.

## 3. Per-driver capability snapshot

One recent public report per vendor/driver/GPU family. "Y" means the extension is listed **and**
its main feature bit is `true` in the report; "N" means the extension is not listed. Fast link =
`graphicsPipelineLibraryFastLinking`. Report URLs are
`https://vulkan.gpuinfo.org/displayreport.php?id=<id>`.

| # | Vendor / driver | GPU (arch) | OS | Driver | Report id | Report date | GPL | Fast link | Shader object | Pipeline binary | Barycentric | Mesh / task |
|---|-----------------|------------|----|--------|-----------|-------------|-----|-----------|---------------|-----------------|-------------|-------------|
| R1 | NVIDIA proprietary | RTX 3070 (Ampere) | Windows | 621.01 | 52954 | 2026-10-31 | Y | true | Y | Y | Y | Y / Y |
| R2 | NVIDIA proprietary | RTX 3070 (Ampere) | Linux | 615.78.08 | 52447 | 2026-10-11 | Y | true | Y | Y | Y | Y / Y |
| R3 | NVIDIA proprietary | GTX 1660 (Turing) | Windows | 616.92 | 53347 | 2026-11-13 | Y | true | Y | Y | Y | Y / Y |
| R4 | NVK (Mesa) | RTX 4070 (Ada) | Linux | Mesa 26.3.0-git (c43cac7264b) | 50382 | 2026-07-23 | Y | true | Y | Y | Y | Y / Y |
| R5 | AMD proprietary | RX 9070 XT (RDNA4) | Windows | 26.10.1 (2.0.416) | 52811 | 2026-10-27 | Y | true | Y | Y | Y | Y / Y |
| R6 | AMD proprietary | RX 6600 (RDNA2) | Windows | 25.10.30.02 (2.0.353) | 44151 | 2025-11-23 | Y | true | Y | Y | Y | Y / Y |
| R7 | RADV (Mesa) | RX 9070 XT (RDNA4) | Linux | Mesa 26.2.6 | 53147 | 2026-11-06 | Y | true | Y | Y | Y | Y / Y |
| R8 | RADV (Mesa) | RX 6650 XT (RDNA2) | Linux | Mesa 26.2.5 | 53353 | 2026-11-13 | Y | true | Y | Y | Y | Y / Y |
| R9 | Intel proprietary | Arc B580 (Xe2) | Windows | 101.9175 | 53104 | 2026-11-05 | Y | true | **N** | **N** | Y | Y / Y |
| R10 | Intel proprietary | Iris Xe (Xe-LP) | Windows | 101.7092 | 52184 | 2026-09-30 | Y | true | **N** | **N** | **N** | **N** |
| R11 | ANV (Mesa) | Arc B580 (Xe2) | Linux | Mesa 26.2.5 | 52762 | 2026-10-25 | Y | true | Y | Y | Y | Y / Y |
| R12 | Qualcomm proprietary | Adreno 830 | Android | 512.891.11 | 53308 | 2026-11-11 | Y | true | **N** | Y | **N** | **N** |
| R13 | Turnip (Mesa) | Adreno 750 | Android | Mesa 26.0.0-devel | 45905 | 2026-01-29 | Y | true | **N** | **N** | **N** | **N** |
| R14 | MoltenVK | Apple M4 | macOS | MoltenVK 1.4.2 | 53270 | 2026-11-10 | **N** | — | **N** | **N** | Y | **N** |
| R15 | KosmicKrisp (Mesa) | Apple M4 | macOS | vulkan-sdk-1.4.363 (git-269a955c89) | 53117 | 2026-11-05 | **N** | — | **N** | **N** | **N** | **N** |

Notes:
- R4 is a Mesa development build; the newest NVK report found for this GPU. Released Mesa
  advertises NVK mesh shaders from 26.2.0 (§7). Mesa 24.1.0's changelog contains the commit
  "nvk: Implement VK_EXT_shader_object", an implementation commit, not a confirmed advertisement.
  Whether released Mesa advertises GPL, shader objects, pipeline binary and barycentrics on NVK
  was not checked: UNKNOWN.
- R13 is the newest Turnip report found for Adreno 750 and is ten months old; a 26.x release may
  differ: UNKNOWN.
- R6 is the newest RDNA2 proprietary report found; it is a year old.
- Not covered: AMDVLK on Linux (no recent reports looked up), Intel Arc A-series on Windows, Mali
  (no target), Honeykrisp (Asahi Linux) — UNKNOWN.

## 4. Subgroup size control: can 32 be forced?

`subgroupSizeControl` is `true` and `VK_EXT_subgroup_size_control` is listed in every report
above. What matters for emulating 32-wide guest warps is the size range and the stages where
`requiredSubgroupSize` may be set (`requiredSubgroupSizeStages`).

| # | Driver | min / max | Default `subgroupSize` | Stages that accept a required size | Can force 32? |
|---|--------|-----------|------------------------|------------------------------------|---------------|
| R1–R3 | NVIDIA proprietary | 32 / 32 | 32 | vertex, tess control/eval, geometry, fragment, compute, ray tracing, task, mesh | Always 32 |
| R4 | NVK | 32 / 32 | 32 | none (`0`) | Always 32 (fixed size) |
| R5 | AMD proprietary 26.10.1 | 32 / 64 | 64 | fragment, compute, task, mesh | Yes in those stages; **not** vertex/tess/geometry |
| R6 | AMD proprietary 25.10.30 | 32 / 64 | 64 | value `32` = compute bit only | Compute only in that driver |
| R7–R8 | RADV 26.2 | 32 / 64 | 64 | fragment, compute, task, mesh | Yes in those stages; **not** vertex/tess/geometry |
| R9 | Intel Windows (B580) | 16 / 32 | 32 | compute, task, mesh | Compute/task/mesh only |
| R10 | Intel Windows (Iris Xe) | 8 / 32 | 32 | compute | Compute only |
| R11 | ANV (B580) | 16 / 32 | 32 | compute, task, mesh | Compute/task/mesh only |
| R12 | Adreno proprietary | 64 / 64 | 64 | compute, task, mesh | **No** (minimum 64) |
| R13 | Turnip | 64 / 128 | 128 | all bits (`2147483647`) | **No** (minimum 64) |
| R14 | MoltenVK | 4 / 32 | 32 | none | Not requestable; reported default 32 |
| R15 | KosmicKrisp | 32 / 32 | 32 | none | Always 32 (fixed size) |

Database-wide distribution (all platforms, number of device models; property pages
`displayextensionproperty.php?extensionname=VK_EXT_subgroup_size_control&extensionproperty=minSubgroupSize&platform=all`
and `...maxSubgroupSize...`, read 2026-11-13; these pages show no "last updated" timestamp, so
the read date is the only date; DOC-MEASURED):
`minSubgroupSize` — 1: 53, 4: 144, 8: 471, 16: 879, 32: 1094, 64: 1734, 128: 2;
`maxSubgroupSize` — 1: 53, 4: 88, 8: 266, 16: 856, 32: 789, 64: 1228, 128: 1064.
These count device models across all report versions, not users, and have no vendor breakdown.

Stages outside `requiredSubgroupSizeStages` (for example vertex, tessellation and geometry on
AMD and Intel) are governed by the specification, not by the reports. Vulkan specification §9.5.2
"Varying Subgroup Size" and the `SubgroupSize` built-in
(https://registry.khronos.org/vulkan/specs/latest/html/vkspec.html, read 2026-11-13; DOC):

- If varying subgroup sizes are not allowed, `SubgroupSize` equals the device's `subgroupSize`
  property.
- Varying sizes are allowed when the SPIR-V module is version 1.6 or later, or when
  `VK_PIPELINE_SHADER_STAGE_CREATE_ALLOW_VARYING_SUBGROUP_SIZE_BIT` (or
  `VK_SHADER_CREATE_ALLOW_VARYING_SUBGROUP_SIZE_BIT_EXT` for shader objects) is set.
- When varying is allowed, stages other than task, mesh and compute may use a different size
  even within a single command.

So on AMD (`subgroupSize` 64), vertex, tessellation and geometry shaders built from SPIR-V 1.5 or
earlier without the varying flag run with `SubgroupSize` = 64; with SPIR-V 1.6 they may vary.
Whether the hardware wave actually matches the reported value is not stated by any public
source read: UNKNOWN.

## 5. Pipeline-creation performance (beyond capability bits)

The tables say what is advertised, not what it costs. PoC-5 (T-012) measures creation and link
latency, but its report is not merged yet, so this note cites no PoC-5 figures: **PoC-5 results
pending (T-012)**. Per ADR-0007 the per-driver override table must be filled by PoC-5 runs on each
vendor (NVIDIA, AMD proprietary, RADV, ANV, NVK, Intel Windows), not by this note.

## 6. Database-wide coverage

Share of **device models** in the database that list each extension (not share of users or of
installed GPUs). `https://vulkan.gpuinfo.org/listextensions.php?platform=<p>`, footer "Last
updated at 2026-11-13 10:20:22 UTC". DOC-MEASURED.

| Extension | All (2357 devices) | Windows (532) | Linux (437) | Android (1447) | macOS (77) |
|-----------|-------------------:|--------------:|------------:|---------------:|-----------:|
| `VK_EXT_graphics_pipeline_library` | 39.03% | 71.62% | 89.93% | 18.66% | 20.78% |
| `VK_EXT_shader_object` | 23.17% | 57.14% | 82.15% | 0.48% | 20.78% |
| `VK_KHR_pipeline_binary` | 28.77% | 53.2% | 71.4% | 12.72% | not listed |
| `VK_KHR_fragment_shader_barycentric` | 20.15% | 41.35% | 40.05% | 6.7% | 55.84% |
| `VK_EXT_subgroup_size_control` | 90.37% | 93.61% | 97.71% | 87.21% | 100% |
| `VK_EXT_mesh_shader` | 19.81% | 49.06% | 47.6% | 5.87% | 22.08% |

Caveats: the per-platform device counts are the footers of each page; the "All" column's device
total (2357) is smaller than the sum of the platform totals, so the site counts devices in a way
this note does not reproduce: UNKNOWN. Which macOS drivers account for the 20.78% GPL/shader-object
rows is UNKNOWN (neither R14 nor R15 lists them). GPL fast-link property, all platforms
(`displayextensionproperty.php?extensionname=VK_EXT_graphics_pipeline_library&extensionproperty=graphicsPipelineLibraryFastLinking&platform=all`,
read 2026-11-13; the page shows no "last updated" timestamp): `true` on 1236 device models,
`false` on 110.

## 7. When each driver started advertising (release notes)

| Driver | Extension | First advertised in | Date | Source | Level |
|--------|-----------|---------------------|------|--------|-------|
| NVIDIA beta | GPL | Windows 473.33 / Linux 470.62.26 | 2022-03-29 | https://developer.nvidia.com/vulkan-driver | DOC |
| NVIDIA beta | fragment barycentric (KHR) | Windows 473.64 / Linux 470.62.30 | 2022-05-24 | same | DOC |
| NVIDIA beta | mesh shader (EXT) | Windows 517.22 / Linux 515.49.15 | 2022-09-01 | same | DOC |
| NVIDIA beta | shader object | Windows 531.54 / Linux 525.47.18 | 2023-03-31 | same; also https://www.khronos.org/blog/you-can-use-vulkan-without-pipelines-today | DOC |
| NVIDIA beta | pipeline binary | Windows 553.00 / Linux 550.40.70 | 2024-08-23 | same | DOC |
| RADV | GPL | Mesa 23.1.0 | 2023-05-10 | https://docs.mesa3d.org/relnotes/23.1.0.html | DOC |
| RADV | mesh shader (GFX11) | Mesa 23.1.0 | 2023-05-10 | same | DOC |
| RADV | shader object | Mesa 24.1.0 | 2024-05-22 | https://docs.mesa3d.org/relnotes/24.1.0.html | DOC |
| RADV | pipeline binary | Mesa 24.3.0 | 2024-11-21 | https://docs.mesa3d.org/relnotes/24.3.0.html | DOC |
| NVK | mesh shader | Mesa 26.2.0 | 2026-08-05 | https://docs.mesa3d.org/relnotes/26.2.0.html | DOC |
| AMDVLK | GPL | v-2024.Q1.2 | 2024-02-23 | https://github.com/GPUOpen-Drivers/AMDVLK/releases?page=2 | DOC |
| AMDVLK | pipeline binary | v-2024.Q3.3 | 2024-09-30 | https://github.com/GPUOpen-Drivers/AMDVLK/releases | DOC |
| MoltenVK | fragment barycentric (KHR) | 1.1.10 | 2022-06-06 | https://github.com/KhronosGroup/MoltenVK/blob/main/Docs/Whats_New.md | DOC |

Everything not in this table (AMD Adrenalin, Intel Windows, Qualcomm, ANV and NVK for the other
extensions, Turnip) has no first-release date from a public source in this pass: UNKNOWN. The
AMDVLK GPL date is the earliest found on the release pages read; an earlier release may exist.

## 8. Mapping onto ADR-0007 tiers

Only the six surveyed features are checked here. ADR-0007's V3/V2/V1 also require extended
dynamic state, descriptor indexing/buffer and buffer device address, which this note does **not**
survey; a "V3" below means "the surveyed part of V3 is met".

| Driver | Pipeline model available | Barycentric | Pipeline binary | Subgroup 32 | Tier (surveyed part) |
|--------|--------------------------|-------------|-----------------|-------------|----------------------|
| NVIDIA proprietary (R1–R3) | GPL + fast link, shader object | Y | Y | always | **V3** |
| NVK (R4, dev build) | GPL + fast link, shader object | Y | Y | always | **V3** (released-Mesa status UNKNOWN) |
| AMD proprietary (R5, R6) | GPL + fast link, shader object | Y | Y | frag/comp/task/mesh (R5); compute only (R6) | **V3** (GPL vs shader objects: PoC-5 results pending) |
| RADV (R7, R8) | GPL + fast link, shader object | Y | Y | frag/comp/task/mesh | **V3** |
| ANV (R11) | GPL + fast link, shader object | Y | Y | comp/task/mesh | **V3** |
| Intel Windows, Arc (R9) | GPL + fast link | Y | **N** | comp/task/mesh | **V2** (fails V3 on pipeline binary) |
| Intel Windows, Xe-LP (R10) | GPL + fast link | **N** | **N** | compute | **V2** (Tier 0 variants without barycentrics) |
| Qualcomm proprietary (R12) | GPL + fast link | **N** | Y | no (≥64) | **V2** at best; not an ADR-0003 target |
| Turnip (R13) | GPL + fast link | **N** | **N** | no (≥64) | **V2** at best; not an ADR-0003 target |
| MoltenVK (R14) | monolithic only | Y | **N** | not requestable | **V1** at best (other V1 requirements not checked) |
| KosmicKrisp (R15) | monolithic only | **N** | **N** | fixed 32 | **V1** at best (other V1 requirements not checked) |

Mesh/task shaders are a separate flag in ADR-0007; they are present on every desktop driver above
except Intel Xe-LP on Windows, and absent on all mobile and Apple drivers sampled.

## 9. Findings

| Source | Observation | Evidence | Applies to Lince? |
|--------|-------------|----------|-------------------|
| gpuinfo R1–R11 | All Windows/Linux desktop drivers sampled advertise GPL with fast link. | DOC-MEASURED | Yes: V2 is the realistic desktop floor; V1 is for old hardware only. |
| gpuinfo R9, R10 | Intel's Windows driver lacks shader objects and pipeline binary; ANV on the same GPU has both. | DOC-MEASURED | Yes: Intel on Windows is V2; the tier is per driver, not per GPU. |
| gpuinfo R5–R11 | Required subgroup size cannot be set for vertex, tessellation or geometry stages on AMD and Intel drivers. | DOC-MEASURED | Yes: input for ADR-0010's warp emulation in those stages. |
| Vulkan spec §9.5.2 | Without varying subgroups (SPIR-V ≤ 1.5, no varying flag) `SubgroupSize` equals `subgroupSize` (64 on AMD); SPIR-V 1.6 allows it to vary outside task/mesh/compute. | DOC | Yes: Lince cannot guarantee 32-wide vertex-pipeline subgroups on AMD; on Intel `subgroupSize` is 32 in R9–R11. |
| gpuinfo R12, R13 | Adreno minimum subgroup size is 64. | DOC-MEASURED | Partially: only if a mobile port is pursued. |
| gpuinfo coverage | Coverage % is per device model, not per user. | DOC-MEASURED | Yes: do not use it as a user-share estimate; user share is UNKNOWN. |

## 10. Open questions

1. Speed and correctness of GPL, shader objects and pipeline binaries on NVIDIA, RADV, ANV, NVK
   and Intel Windows — capability bits do not answer this; PoC-5 runs on each are needed.
2. Whether the hardware wave in stages outside `requiredSubgroupSizeStages` (AMD/Intel vertex,
   tessellation, geometry) actually matches the reported `SubgroupSize`: UNKNOWN.
3. Released-Mesa NVK and current Turnip feature sets (R4 is a git build, R13 is from January).
4. User share per tier: no public source found; gpuinfo counts device models. UNKNOWN.
5. The ADR-0007 requirements not surveyed here (EDS1/2/3, descriptor indexing/buffer, BDA) — a
   follow-up pass could use the same method.

## 11. Affected ADRs, PoCs and tasks

- **ADR-0007:** the tier table is consistent with the data; Intel Windows lands in V2 because of
  the pipeline-binary requirement. Whether V3 should require pipeline binary or treat it as a
  separate flag is a maintainer decision, not made here.
- **ADR-0013:** the choice between GPL and shader objects per driver waits for PoC-5 results
  (T-012); this note only shows that both are advertised on most desktop drivers.
- **ADR-0010:** forcing 32-wide subgroups is possible only in some stages on AMD/Intel and never on
  Adreno; AMD vertex-pipeline stages run at 64 unless SPIR-V 1.6 varying sizes apply (§4).
- **PoC-5 / T-012:** measurements are needed on every desktop driver in §3, not only AMD.
