# Backlog

The task queue for humans and agents. The `next-task` skill reads this file.

**Statuses:** `Ready` · `In progress` · `In review` · `Needs decision` · `Blocked` · `Done`
**Types:** `Maintainer` (only the maintainer can do it) · `Research` · `Docs` · `PoC` · `Impl`

Rules:
- `Impl` tasks need all listed ADRs **Accepted**. `PoC`, `Research` and `Docs` tasks may run
  against `Proposed` ADRs because they produce the evidence those ADRs need.
- Keep one row per task in the table; details go in the section below. Agents update the
  `Status` column and append the PR link in the details.

| ID | Title | Type | Status | Depends on | ADRs |
|----|-------|------|--------|------------|------|
| T-001 | Choose the project license | Maintainer | Done | — | 0000 |
| T-002 | Review ADRs ready for acceptance (0001 details, 0002, 0014) | Maintainer | Done | — | 0001, 0002, 0014 |
| T-003 | Bootstrap Cargo workspace, xtask and CI skeleton | Impl | In review | T-001, T-002 | 0001, 0002 |
| T-004 | Commit linting in CI and changelog generation (git-cliff) | Impl | Blocked | T-003 | 0002 |
| T-005 | Reference crate: `lince-common` written to every convention | Impl | Blocked | T-003 | 0001 |
| T-006 | Decide whether to require DCO `Signed-off-by` on commits | Maintainer | Needs decision | — | 0000 |
| T-010 | PoC-6: cache store prototype and benchmark harness | PoC | Done | — | 0015, 0016 |
| T-011 | PoC-7: frame ledger prototype | PoC | Done | — | 0027 |
| T-012 | PoC-5: Vulkan pipeline mechanisms harness | PoC | In review | — | 0007, 0013 |
| T-013 | Tier-0 micro-op format specification | Docs | Done | — | 0010 |
| T-014 | Ampere SASS test corpus: compile team-written CUDA kernels (no NVIDIA GPU needed) | PoC | Needs decision | — | 0009, 0028 |
| T-015 | Research: inspect oboromi, Pound, Hokkaido, Anka, Orchid, Eden | Research | In review | — | — |
| T-016 | Research: Vulkan feature coverage survey per vendor/driver (GPL, shader objects, pipeline binary, barycentrics, subgroup size control, mesh) | Research | In review | — | 0007 |
| T-019 | Research: system data and applets Switch 2 titles depend on (firmware-free replacements) | Research | In review | — | 0024, 0029 |
| T-017 | PoC-2 step 1: compute-only GPU interpreter harness | PoC | Blocked | T-013, T-014 | 0010 |
| T-018 | PoC-4 step 1: Ampere SASS decoder skeleton + encoding notes | PoC | Blocked | T-014 | 0009 |
| T-020 | PoC-6: XXH3 payload checksum and CRC-32C vs XXH3 timing | PoC | Ready | T-010 | 0015 |
| T-021 | PoC-7: loom model of the ledger ring and ARM test run | PoC | Ready | T-011 | 0027 |
| T-022 | PoC-5: measure `VK_KHR_pipeline_binary` once `ash` supports it | PoC | Blocked | T-012 | 0013 |
| T-023 | Optional: SASS corpus reference outputs on an NVIDIA RTX 30/40 GPU | Maintainer | Blocked | T-014 | 0028 |
| T-024 | PoC-6: time each engine's reader in another process (`redb` in `SingleWriter` mode) | PoC | Ready | T-010 | 0015 |
| T-025 | Record the hybrid Rust + C++ decision (ADR-0030) and the planned D3D12/Metal backends; update docs, rules and hooks | Docs | In review | — | 0006, 0007, 0009, 0030 |
| T-026 | PoC-11: Rust↔C++ boundary (`cc` build, C header bindings, call cost, C++ checks in `xtask ci`) | PoC | Ready | T-025 | 0030 |

## Details

### T-001 — Choose the project license
Owner decision on ADR-0000. Agents must not pick a license.
- **Done 2026-11-13:** the owner chose GPL-3.0-or-later; ADR-0000 is Accepted.

### T-002 — Review ADRs ready for acceptance
Maintainer reviews ADR-0001 (details), ADR-0002 and ADR-0014 and changes their status.
- **Done 2026-11-13:** the maintainer accepted ADR-0001, ADR-0002 and ADR-0014 in full.

### T-003 — Bootstrap Cargo workspace, xtask and CI skeleton
- Workspace `Cargo.toml` with shared lints and profiles (doc 04 §7); `rust-toolchain.toml`
  pinned to current stable; `xtask` with `ci` (fmt, clippy `-D warnings`, test, invariant
  check, license headers).
- Only `lince-common` and `lince-trace` crates created (empty APIs with docs and one test each).
- GitHub Actions: Windows x64 + Linux x64 running `cargo xtask ci`.
- Apply `[workspace.lints]` from `docs/conventions/lints.md`; root `rustfmt.toml`,
  `clippy.toml` and `deny.toml` already exist; `xtask ci` runs `cargo deny check` and verifies
  SPDX headers (LEGAL-30) and `license` fields (LEGAL-31).
- **Acceptance:** clean checkout builds and `cargo xtask ci` passes on both OSes.
- **PR:** https://github.com/lince-emu/lince/pull/11

### T-005 — Reference crate `lince-common`
Write `lince-common` (newtypes such as `GuestVa`/`GpuVa`/`Nanos`, a small error enum, one
platform-neutral helper only if three crates need it) as the model every later crate copies:
crate template (STRUCT), module docs, patterns PAT-01/02/03, tests (QUAL-70–73), lints clean.
- **Acceptance:** maintainer reviews it explicitly as the reference; `docs/conventions/patterns.md`
  links to its real implementations.

### T-006 — DCO decision
Decide whether commits must carry `Signed-off-by` (Developer Certificate of Origin, LEGAL-35).
If yes, extend `.githooks/commit-msg` and `docs/conventions/commits.md`.

### T-004 — Commit linting in CI and changelog generation
- `cargo xtask ci` validates every commit in the PR range with the same rules as
  `.githooks/commit-msg` (`docs/conventions/commits.md` §9), and the PR title.
- `cliff.toml` for `git-cliff` grouping by type as in `docs/conventions/versioning.md` §4.
- **Acceptance:** CI fails on a non-conforming commit or PR title; `git cliff` produces a
  grouped `CHANGELOG.md` from the existing history.

### T-010 — PoC-6 cache store
Follow `docs/06-poc-roadmap.md` PoC-6 and `docs/02-cache-architecture.md` §4–§9.
- Custom store (segments + mmap index), `redb` comparator, SQLite comparator behind a feature.
- Synthetic workload generator (blob sizes 1 KiB–2 MiB, configurable distribution).
- Fault-injection tests (truncation, bit flips, kill during append).
- **Acceptance:** harness produces open-time vs. size and resolve-latency CSVs; report skeleton
  in `docs/poc-reports/poc-6-cache-store.md`; correctness tests pass. Measurements on the
  maintainer's hardware are a follow-up.
PR: https://github.com/lince-emu/lince/pull/2

### T-011 — PoC-7 frame ledger
Per-thread lock-free ring of fixed-size events, dump format, minimal analyzer computing
percentiles and attributing synthetic injected spikes.
- **Acceptance:** event cost microbenchmark; analyzer test with synthetic spikes identifies
  the injected cause.
PR: https://github.com/lince-emu/lince/pull/3

### T-012 — PoC-5 Vulkan pipeline mechanisms
Standalone `ash` harness that creates pipelines from generated SPIR-V of several sizes using:
monolithic, GPL non-LTO + fast link, GPL LTO, shader objects, sharded `VkPipelineCache`,
`DISABLE_OPTIMIZATION`. Detects support at runtime and skips unsupported paths. Pipeline
binary load is deferred to T-022, because `ash` 0.38 has no `VK_KHR_pipeline_binary` bindings.
- **Acceptance:** runs on any Vulkan 1.3 device, writes per-mechanism latency CSV; report skeleton.
- **PR:** https://github.com/lince-emu/lince/pull/14

### T-013 — Tier-0 micro-op format specification
Write `docs/research/tier0-micro-op-format.md`: encoding, operand descriptors, register-file
buckets, branch target resolution, how predicates/uniform registers are represented, and how
the GPU interpreter loop consumes it (doc 03 §4). Propose as input for ADR-0010; no code.
PR: https://github.com/lince-emu/lince/pull/1

### T-014 — SASS corpus
Decision [D-005](decisions/D-005-sass-corpus-without-nvidia-gpu.md). Write a set of
team-written CUDA kernels, compile them for `sm_86` and store the SASS (`cuobjdump`/`nvdisasm`).
This does not need an NVIDIA GPU. Prerequisite: the maintainer installs the CUDA toolkit
without the driver. Tools are used only as LEGAL-14 allows. Also write the one-command script
that T-023 runs, and expected outputs computed on the CPU for each kernel.
- **Acceptance:** the corpus builds from a clean checkout with one command; every kernel has
  its SASS and its CPU-computed expected output stored; the T-023 script runs to the point
  where it needs a GPU.
- **Needs decision:** [D-010](decisions/D-010-cuda-eula-sass-corpus.md) — the CUDA EULA
  (§1.2 item 8) may forbid disassembling `nvcc` output for translation to non-NVIDIA GPUs.

### T-023 — SASS corpus reference outputs (optional)
Run the T-014 script once on an NVIDIA GPU of compute capability 8.6–8.9 (GeForce RTX 30 or
40 series; an `sm_86` cubin runs only on compute capability 8.6 and later 8.x devices), and
commit the outputs. Anyone with such a GPU can run it. Nothing depends on this task: PoC-2
and PoC-4 use the doc 06 matching rule, and these outputs only tighten it.

### T-015 — Research: other Switch 2 projects
Use the `research-note` skill. Focus: language, CPU backend, shader approach, cache design,
any Rust crates worth evaluating. Pin commits.

### T-016 — Research: Vulkan feature coverage survey
Use public sources (gpuinfo database, Mesa release notes, vendor driver notes). Output a table
per vendor/driver for the features in ADR-0007's tiers; mark data dates.
PR: https://github.com/lince-emu/lince/pull/10

### T-019 — Research: firmware-free system data and applets
Use the `research-note` skill. From public documentation and the service calls observed by
other projects (never from firmware files or leaked material), list the system data archives
and applets Switch 2 titles use, and for each propose an original implementation, a freely
licensed substitute, or a documented "unavailable" behavior. Extend
`docs/10-firmware-independence.md` §3.
- **In review:** [lince-emu/lince#9](https://github.com/lince-emu/lince/pull/9); open decision [D-009](decisions/D-009-shared-font-substitutes.md).

### T-017 / T-018
Start after their dependencies; see PoC-2 and PoC-4 in `docs/06-poc-roadmap.md`.
T-018: decision [D-006](decisions/D-006-oboromi-sass-tables-off-limits.md) must be answered first.

### T-020 — PoC-6 XXH3 checksum
Decision [D-001](decisions/D-001-cache-hash-xxh3.md). Add `xxhash-rust` (`xxh3` feature) to
`poc/cache-store`, use XXH3-64 for the payload checksum as doc 02 §4.1 specifies, and time
CRC-32C vs XXH3-64 on 1 KiB–2 MiB payloads in the harness.
- **Acceptance:** harness writes a checksum-cost CSV; fault-injection tests still pass; report
  updated (simplification 1 removed or measured).

### T-021 — PoC-7 memory-model checks
Decision [D-004](decisions/D-004-ledger-memory-model-checks.md). Add `loom` as a
dev-dependency of `poc/frame-ledger` and model one producer and one concurrent snapshot;
run the test suite on an ARM host. Until an ARM CI runner exists (T-003 sets up x64 runners
only), use any ARM host available to the maintainer or a contributor.
- **Acceptance:** the loom test passes, and it fails when either of these fences in
  `poc/frame-ledger/src/ring.rs` is removed (shown in the PR): the release fence in
  `Ring::push` and the acquire fence in `Ring::snapshot`. If loom cannot detect the removal
  of one of them, the PR and the PoC-7 report document that loom limitation instead. The ARM
  run result, with the host it ran on, is recorded in the PoC-7 report.

### T-022 — PoC-5 pipeline binaries
Blocked until an `ash` release includes `VK_KHR_pipeline_binary` (0.38 predates it). Then add
pipeline binary load to the PoC-5 harness and the report. Rationale: ADR-0013 relies on
pipeline binaries for L6 persistence, and PoC-5's hypothesis that their loads are
sub-millisecond is untested until they are measured.
- **Acceptance:** the PoC-5 harness times pipeline binary load next to the other mechanisms,
  skips it where the extension is unsupported, and the PoC-5 report records the results.

### T-024 — PoC-6 second-process readers
Decision [D-002](decisions/D-002-cache-engine-second-process.md). Let
`harness resolve --writer on` run the reader in a separate process while the writer appends,
for every engine: the custom store, `redb` opened with `ConcurrencyMode::SingleWriter`
(writer and reader in that mode) and SQLite in WAL mode. Record whether each engine serves
that reader, which ADR-0015's "Conditions for revisiting" requires, so the "within 20%"
comparison uses the same reader setup for all engines.
- **Acceptance:** `harness resolve --writer on` times a reader in another process for each
  engine; the PoC-6 report states, per result row, the reader setup and the `redb` mode, and
  whether each engine served the second-process reader.

### T-025 — Hybrid Rust + C++ decision
The maintainer decided on 2026-11-13 that the guest CPU and shader subsystems are written in
C++ behind a C ABI, with Rust owning the rest. Record it as ADR-0030 (superseding ADR-0026 and
the language clause of ADR-0001) and update the charter, doc 04, ADR-0004/0005, the
`ARCH`/`LEGAL`/`DEV` rules, `AGENTS.md`, the agents and `check-invariants.sh`. Also record
that Vulkan is the first of several backends (Direct3D 12 and Metal later): ADR-0006/0007/0009,
backend in the doc 02 cache keys, ARCH-28.
- **Acceptance:** no document still states that Lince is Rust only; the invariant hook flags
  C++ outside `[c++]` crates, exceptions/RTTI, threads and I/O in C++, and missing SPDX headers
  in C++ files.

### T-026 — PoC-11 Rust↔C++ boundary
Follow `docs/06-poc-roadmap.md` PoC-11 and ADR-0030. Standalone crate in `poc/cpp-boundary/`.
- C++20 static library built by `build.rs` with the `cc` crate (`clang-cl` on Windows, `clang`
  on Linux); one C header; Rust declarations generated from it and checked in, with a CI check
  that they match; layout tests for every boundary struct.
- Microbenchmarks: Rust→C++ call and C++→Rust callback vs. a non-inlined Rust call.
- `xtask ci` gains `clang-format`, `clang-tidy` (warnings as errors), ASan/UBSan runs of the
  C++ tests, and the SPDX check (LEGAL-30) for `.cpp`, `.hpp` and `.h` files.
- New dependencies (`cc`, and `bindgen` if chosen) are listed with license and reason
  (DEV-80).
- **Acceptance:** `cargo xtask ci` passes from a clean checkout on Windows x64 and Linux x64;
  report in `docs/poc-reports/poc-11-cpp-boundary.md` with the measured call costs, build
  times, chosen binding generator and minimum clang version.
