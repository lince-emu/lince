# 08 — Switch 2 Hardware and System Interfaces: Known, Inferred, Unknown

Status: Living document · Last reviewed: 2026-11-13

Confidence levels: **[Official]** Nintendo/NVIDIA statement or widely confirmed spec sheet ·
**[RE]** reverse-engineering documentation (switchbrew) · **[EMU]** observed by an emulator
project, reproducible but not independently verified by Lince-NX · **[Inferred]** engineering
inference · **[Unknown]**.

## 1. SoC (NVIDIA T239)

| Item | Value | Confidence |
|------|-------|------------|
| CPU | 8 × Arm Cortex-A78C, AArch64 only (no 32-bit in the SDK) | Official (spec reports), RE |
| CPU cores for applications | 6 (2 reserved for the system) | Official (spec reports); EMU (NPDM core mask `0x3f`) |
| CPU clocks | ~998 MHz docked, ~1101 MHz handheld, up to ~1.7 GHz | Official (spec reports) |
| Caches | 64 KiB L1I + 64 KiB L1D per core, 256 KiB L2 per core, 4 MiB shared L3 | RE (switchbrew) |
| ISA features in shipped code | LSE atomics, CAS/CASP, LDAPR (RCpc), PAC (RETAA/RETAB, PACGA, XPAC…), MTE `STG`, FP16/v8.2 SIMD | EMU (Eglantine/Lichii coverage scans) |
| GPU | NVIDIA Ampere, 12 SMs, 1536 CUDA cores; ~561 MHz handheld, ~1007 MHz docked, max ~1.4 GHz | Official (spec reports), RE |
| Tensor cores | Present (Ampere); titles ship tensor-core networks (DLSS-style) | Official (DLSS support), EMU (HMMA programs observed) |
| Memory | 12 GB LPDDR5X, 128-bit; 9 GB for applications reported | Official (spec reports), RE |
| Guest address space | 39-bit for tested titles | EMU |

## 2. GPU programming interface

| Item | Observation | Confidence |
|------|-------------|------------|
| API | NVN2 (proprietary; no public documentation) | Official (name), EMU |
| Classes | 3D `C797`, compute `C7C0`, DMA `C7B5`, inline-to-memory on subchannel 2 | EMU |
| Macros | Turing-style MME64; drive most 3D/compute state and draws | EMU |
| Compute launch | QMD 3.0; queue-to-queue syncs through QMD release slots | EMU |
| Shader ISA | Ampere SASS (labelled SM86 by emulator projects) — exact SM variant of T239 | EMU; exact variant **[Unknown]** |
| Mesh/task shaders | Used by shipped titles | EMU |
| GPU VA model | GPU VA equals CPU VA for registered memory; sparse regions; read-only syncpoint shim page | EMU |
| Command buffers | Semaphore-gated; GPFIFO entries may end mid-method; interleaved command buffers | EMU |

## 3. System services (selection)

Switch 2 adds or changes services seen by emulator projects: `gsv` (graphics system,
including GPU, syncpoint and vi2 sessions), `vi2` presentation, `nvsrv` (nvdrv protocol),
`aud:u` (audio entry), new `appletOE`/`IApplicationProxy` commands, new `svcGetInfo` ids and SVC
`0x80` (GPU memory attributes). All protocol details are **[EMU]** and partially guessed by those
projects (Lichii `NX2_NOTES.md` "Prioritized next steps" item 3 lists guesses still to replace).

## 4. Unknown or weakly known (must not be invented)

- Exact SASS encoding coverage for every instruction used by NVN2-compiled shaders.
- Precise semantics of undocumented GPU registers and macro behaviors.
- Real protocol of several `gsv` commands, syncpoint page layout, buffer-release semantics for
  presentation.
- Video decoder interfaces beyond what is needed for VP9/H.264 movies.
- Timing behavior relied on by games (CPU/GPU relative speed, vsync behavior).

## 5. Implications for Lince-NX

1. A64 decoder covers v8.0–v8.5 features listed above from day one (ADR-0004).
2. Emulate 6 application cores exactly (doc 01 E3).
3. Ampere-native shader front-end, no lowering to an older ISA (ADR-0009).
4. MME64 is a first-class engine with a JIT path (ADR-0008).
5. Tier-0 coverage must be measured against mesh/task and tensor programs (doc 03 §6).

## Sources

- [switchbrew: Switch 2 Tegra T239](https://switchbrew.org/wiki/Switch_2:_Tegra_T239)
- [TechSpot: Switch 2 tech specs](https://www.techspot.com/news/107922-nintendo-finally-clarifies-switch-2-custom-nvidia-processor.html)
- [Nintendo Life: final tech specs](https://www.nintendolife.com/news/2025/05/nintendo-switch-2-final-tech-specs-have-been-confirmed)
- [Lichii `NX2_NOTES.md`](https://github.com/zortos293/Lichii/blob/nx2/NX2_NOTES.md) and [Eglantine `NX2_NOTES.md`](https://github.com/Eglantine-Community/Eglantine/blob/nx2/NX2_NOTES.md)
