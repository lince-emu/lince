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
| T-003 | Bootstrap Cargo workspace, xtask and CI skeleton | Impl | Done | T-001, T-002 | 0001, 0002 |
| T-004 | Commit linting in CI and changelog generation (git-cliff) | Impl | Done | T-003 | 0002 |
| T-005 | Reference crate: `lince-common` written to every convention | Impl | Done | T-003 | 0001 |
| T-006 | Decide whether to require DCO `Signed-off-by` on commits | Maintainer | Needs decision | — | 0000 |
| T-010 | PoC-6: cache store prototype and benchmark harness | PoC | Done | — | 0015, 0016 |
| T-011 | PoC-7: frame ledger prototype | PoC | Done | — | 0027 |
| T-012 | PoC-5: Vulkan pipeline mechanisms harness | PoC | Done | — | 0007, 0013 |
| T-013 | Tier-0 micro-op format specification | Docs | Done | — | 0010 |
| T-014 | Ampere SASS test corpus: team-written assembler and programs (no NVIDIA SDK or GPU) | PoC | Done | — | 0009, 0028 |
| T-015 | Research: inspect oboromi, Pound, Hokkaido, Anka, Orchid, Eden | Research | Done | — | — |
| T-016 | Research: Vulkan feature coverage survey per vendor/driver (GPL, shader objects, pipeline binary, barycentrics, subgroup size control, mesh) | Research | Done | — | 0007 |
| T-019 | Research: system data and applets Switch 2 titles depend on (firmware-free replacements) | Research | Done | — | 0024, 0029 |
| T-017 | PoC-2 step 1: compute-only GPU interpreter harness | PoC | Blocked | T-013, T-014 | 0010 |
| T-018 | PoC-4 step 1: Ampere SASS decoder skeleton + encoding notes | PoC | Blocked | T-014 | 0009 |
| T-020 | PoC-6: XXH3 payload checksum and CRC-32C vs XXH3 timing | PoC | Done | T-010 | 0015 |
| T-021 | PoC-7: loom model of the ledger ring and ARM test run | PoC | Done | T-011 | 0027 |
| T-022 | PoC-5: measure `VK_KHR_pipeline_binary` once `ash` supports it | PoC | Blocked | T-012 | 0013 |
| T-023 | Optional: SASS corpus reference outputs on an NVIDIA RTX 30/40 GPU | Maintainer | Blocked | T-014 | 0028 |
| T-024 | PoC-6: time each engine's reader in another process (`redb` in `SingleWriter` mode) | PoC | Done | T-010 | 0015 |
| T-025 | Record the hybrid Rust + C++ decision (ADR-0030) and the planned D3D12/Metal backends; update docs, rules and hooks | Docs | Done | — | 0006, 0007, 0009, 0030 |
| T-026 | PoC-11: Rust↔C++ boundary (`cc` build, C header bindings, call cost, C++ checks in `xtask ci`) | PoC | Done | T-025 | 0030 |
| T-036 | Study: what Rust and C++ should each own (refines ADR-0030) | Research | Done | T-026 | 0005, 0019, 0030, 0031 |
| T-027 | Review the B1 ADRs for acceptance (0004, 0005 interpreter part, 0019 address-space part, 0024 SVC boundary, 0027 logging part) | Maintainer | Done | — | 0004, 0005, 0019, 0024, 0027 |
| T-028 | Research: NRO format and the SVCs B1 uses (public sources) | Research | In review | — | 0024 |
| T-029 | Test homebrew: build the B1 programs from team-written A64 assembly with LLVM, and an ELF→NRO tool | Impl | Ready | T-028 | 0001, 0002 |
| T-030 | `lince-trace`: log facade for named stubs (no ledger yet) | Impl | In review | T-027 | 0027 |
| T-031 | `lince-mem`: flat guest address space with typed access | Impl | In review | T-027 | 0019 |
| T-032 | `lince-loader`: load an NRO into guest memory | Impl | Ready | T-028, T-031 | 0024 |
| T-033 | `lince-cpu-a64` `[c++]`: decoder and interpreter for the B1 instructions, with differential tests | Impl | Ready | T-027, T-029 | 0004, 0005, 0030 |
| T-034 | `lince-kernel`: SVC dispatch for memory, exit and `OutputDebugString` | Impl | Ready | T-027, T-028 | 0024 |
| T-035 | `lince-app`: `lince run <file>` prints the test NRO's "hello" (B1 exit) | Impl | Ready | T-029–T-034 | 0001 |
| T-040 | `lince-cpu-a64`: full v8.0–v8.5 decode table and `lince-a64-coverage` | Impl | Blocked | T-033 | 0004, 0031 |
| T-037 | Study: host core budget (6 guest cores + role threads + workers on 6–8-core hosts) | Research | Ready | — | 0017, 0005 |
| T-038 | Design: LCIR v1 (minimal CPU IR for Tier B) | Docs | Blocked | T-039 | 0005 |
| T-039 | Research: how other JITs schedule compilation (clean-room, cited) | Research | Ready | — | 0005 |

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
- **PR:** https://github.com/lince-emu/lince/pull/26

### T-006 — DCO decision
Decide whether commits must carry `Signed-off-by` (Developer Certificate of Origin, LEGAL-35).
If yes, extend `.githooks/commit-msg` and `docs/conventions/commits.md`.

### T-004 — Commit linting in CI and changelog generation
- `cargo xtask ci` validates every commit in the PR range with the same rules as
  `.githooks/commit-msg` (`docs/conventions/commits.md` §9), and the PR title.
- `cliff.toml` for `git-cliff` grouping by type as in `docs/conventions/versioning.md` §4.
- **Acceptance:** CI fails on a non-conforming commit or PR title; `git cliff` produces a
  grouped `CHANGELOG.md` from the existing history.
- **PR:** https://github.com/lince-emu/lince/pull/25

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
Decisions [D-005](decisions/D-005-sass-corpus-without-nvidia-gpu.md) and
[D-010](decisions/D-010-cuda-eula-sass-corpus.md). No NVIDIA SDK tool is used (LEGAL-15).
Encoding source: Mesa NAK, per [D-011](decisions/D-011-sass-encoding-source.md).
Standalone crate in `poc/sass-corpus/`:
- a Rust assembler for an initial subset of Ampere (`sm_86`) SASS: integer and logic ops,
  FADD/FMUL/FFMA, moves, predicates, branches, LDG/STG and EXIT;
- every encoding used is recorded in `docs/research/ampere-sass-encoding.md` with its public
  source (LEGAL-11; oboromi's tables only as
  [D-006](decisions/D-006-oboromi-sass-tables-off-limits.md) allows); encodings that no public
  source gives are not guessed into the corpus;
- corpus programs in the assembler's text form, each with deterministic inputs and expected
  outputs computed on the CPU from the program's documented semantics;
- the one-command build that assembles every program and stores its binary and expected
  output, and the T-023 runner, which stops with a clear message where it needs a GPU.
- **Acceptance:** the corpus builds from a clean checkout with one command; every program has
  its binary and its CPU-computed expected output stored; each encoding cites its source; the
  T-023 runner runs to the point where it needs a GPU.
- **PR:** https://github.com/lince-emu/lince/pull/23

### T-023 — SASS corpus reference outputs (optional)
Run the T-014 runner once on an NVIDIA GPU of compute capability 8.6–8.9 (GeForce RTX 30 or
40 series; `sm_86` code runs only on compute capability 8.6 and later 8.x devices), and
commit the outputs. The runner loads the team-assembled programs through the NVIDIA driver;
no SDK tool is used (LEGAL-15). Anyone with such a GPU can run it. Nothing depends on this task: PoC-2
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
T-017 is paused until after B6 (`docs/07-development-roadmap.md`, "What pauses").
T-018: decision [D-006](decisions/D-006-oboromi-sass-tables-off-limits.md) must be answered first.

### T-020 — PoC-6 XXH3 checksum
Decision [D-001](decisions/D-001-cache-hash-xxh3.md). Add `xxhash-rust` (`xxh3` feature) to
`poc/cache-store`, use XXH3-64 for the payload checksum as doc 02 §4.1 specifies, and time
CRC-32C vs XXH3-64 on 1 KiB–2 MiB payloads in the harness.
- **Acceptance:** harness writes a checksum-cost CSV; fault-injection tests still pass; report
  updated (simplification 1 removed or measured).
- **PR:** https://github.com/lince-emu/lince/pull/17

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
- **PR:** https://github.com/lince-emu/lince/pull/19

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
- **PR:** https://github.com/lince-emu/lince/pull/21

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
- **PR:** https://github.com/lince-emu/lince/pull/27

### T-036 — Study: Rust/C++ split
Asked by the maintainer: where each language pays off, given ADR-0030 and PoC-11's measured
boundary cost. Output `docs/research/rust-cpp-split.md`, with three proposed amendments to
ADR-0030 / doc 04 (executable code memory in `lince-mem`, a fault-lookup function in the CPU
header, no separate Rust emitter for the MME64 JIT).
- **Accepted** as ADR-0031 (maintainer, 2026-11-13).
- **PR:** https://github.com/lince-emu/lince/pull/31

### T-027 — B1 ADR review (maintainer)
The boot-first roadmap (doc 07, B1) needs these accepted before its `Impl` tasks start:
ADR-0004 (guest CPU model), ADR-0005 (the interpreter part), ADR-0019 (the guest address-space
part), ADR-0024 (the SVC boundary) and ADR-0027 (the logging part: B1 logs every stub by name;
the frame ledger stays with PoC-7 and its ARM run). Since PR #30 each of these ADRs marks the
part B1 needs as **B1 scope**; accepting means recording that the B1 scope is accepted while
the rest stays Proposed. ADR-0031 (Rust/C++ placement) is already Accepted and applies to
T-031 and T-033.
- **Acceptance:** each ADR's status line states what is accepted; T-030 to T-034 become Ready.
- **Done 2026-11-14:** the maintainer accepted the B1 scope of ADR-0004, 0005, 0019, 0024 and
  0027; the rest of each stays Proposed.

### T-028 — Research: NRO format and B1 SVCs
Use the `research-note` skill. From public sources only (switchbrew and similar, LEGAL-11;
never SDK material, LEGAL-10): the NRO header and segment layout, how a homebrew program
receives its entry arguments, and the numbers, arguments and results of the SVCs B1 uses
(memory query/set, exit, `OutputDebugString`), plus what public sources say about the Switch 2
SVC `0x80` (doc 01 E2), which ADR-0024's SVC table must reserve. Note what is Switch 1
knowledge and what is known to hold on Switch 2 (doc 08).
- **Acceptance:** `docs/research/nro-and-b1-svcs.md` with per-field sources and evidence
  levels; UNKNOWN items listed in doc 08.

### T-029 — Test homebrew programs
The B1 programs are written by the team, never taken from homebrew projects (LEGAL-20/21):
- A64 assembly in `tests/homebrew/` (hello via `OutputDebugString`, then exit; plus one
  program per instruction group the interpreter implements);
- built with LLVM (`clang --target=aarch64-none-elf`, `ld.lld`), already required by ADR-0030;
- a team-written ELF→NRO tool in `tools/` (format from T-028), driven by an `xtask` command,
  with the generated NROs checked in so tests do not need LLVM.
- **Acceptance:** one command rebuilds every NRO bit-for-bit; the NRO header fields are
  checked by a test against T-028's note.

### T-030 — `lince-trace` log facade
Only what B1 needs: a log call that records a named stub or event once per name (no flood),
usable from every crate, with no I/O on render-critical threads (ARCH-03: records go to a
buffer drained elsewhere). Records use the frame ledger's event shape (ADR-0027 B1 scope), so
the ledger promoted from PoC-7 later reads the same data instead of a second log format.
- **Acceptance:** follows the `lince-common` reference layout; tests cover once-per-name and
  draining; no new dependency without approval.

### T-031 — `lince-mem` flat guest address space
The ADR-0019 B1 scope: a 39-bit guest address space of mapped regions with permissions,
typed reads and writes by `GuestVa` (QUAL-20), by software lookup. The layout already follows
ADR-0019's rule for later native execution: one contiguous host reservation whose base and
guest-page-to-host-page mapping are explicit parameters, with no 4 KiB host-page assumption.
The interpreter (T-033) gets the base pointer and reads guest memory inline (ADR-0031).
JIT code memory and W^X (ADR-0031) come with the first JIT tier, not in B1.
- **Acceptance:** tests for mapped, unmapped and permission-faulting accesses; a test with a
  16 KiB host-page configuration; access through `GuestVa` only.

### T-032 — `lince-loader` NRO
Parse the NRO header and map its segments into `lince-mem` with their permissions; decode raw
fields once at the boundary (PAT-03).
- **Acceptance:** loads the T-029 NROs; a corrupt or truncated file is an error, never a panic;
  fuzz target for the header parser (ADR-0028).

### T-033 — `lince-cpu-a64` decoder and interpreter
The first `[c++]` crate (ADR-0030 layout validated by PoC-11): decoder and interpreter for
the instructions the T-029 programs use. The decoder is generated from one instruction
description table (ADR-0004), the only A64 decoder in Lince: the interpreter now and the JIT
tiers later share it, and Rust tools decode through the C API (ADR-0031). An encoding the
table lacks is reported as unknown, never guessed. Guest memory is accessed inline through the
base pointer Rust provides; the callback table is the slow path only (ADR-0031). Tests run with
ADR-0004's deterministic clock. Instruction semantics come from Arm's public specification
(LEGAL-11).
- **Acceptance:** differential tests of every implemented instruction against expected values
  derived from the specification (and against the host CPU on ARM hosts where available);
  `cargo xtask ci` C++ checks pass; a benchmark records interpreter throughput (ns per guest
  instruction per instruction group) on the B1 programs, the baseline for how long code may
  stay in Tier A before Tier B publishes (ADR-0005).

### T-034 — `lince-kernel` SVC dispatch
Dispatch the SVCs from T-028 (memory, exit, `OutputDebugString`) through a table indexed by
SVC number with the Switch 2 SVC `0x80` reserved (ADR-0024 B1 scope). An unknown SVC returns a
distinct error, never success or a panic, and is logged once by name and number through
`lince-trace`. Names come from public documentation or are descriptive (LEGAL-12).
- **Acceptance:** each SVC tested through the dispatch API; unknown SVCs logged once.

### T-035 — `lince run` (B1 exit)
`lince-app` CLI: `lince run <file.nro>` loads the file, runs it on the interpreter and prints
`OutputDebugString` text on the host terminal.
- **Acceptance:** the B1 exit criterion of doc 07, demonstrated: the T-029 hello NRO prints
  "hello"; an integration test runs it in CI on Windows and Linux.

### T-037 — Study: host core budget
ADR-0005 moves all JIT work to `lince-sched` workers, which then compete with the six guest
core threads, the GPU front and back, Vulkan submit, present and audio. On 6–8-core hosts
that is more runnable threads than cores. Study how many threads each role needs, how
priorities (ADR-0017 P0–P4) and the governor share the cores, what happens when the host has
fewer cores than guest cores, and what the ledger must record to see it.
- **Acceptance:** `docs/research/host-core-budget.md` with a proposed policy per host core
  count and the ledger events to validate it; proposed ADR-0017 changes listed, not applied.

### T-038 — LCIR v1 design
Specify the minimal LCIR that Tier B needs for the B1/B2 instruction set (ADR-0005): values,
guest-state access, memory operations with size and ordering, explicit exits, the serialized
form that crosses the C ABI and is hashed for the cache. Written from the Arm specification
and Lince's requirements, after T-039 so known pitfalls are listed without borrowing any
design (LEGAL-20/21).
- **Acceptance:** `docs/design/lcir-v1.md` (or a doc 03-style chapter) the maintainer
  approves before the first JIT tier is implemented.

### T-039 — Research: how other JITs schedule compilation
Use the `research-note` skill. Behavior only, cited by repository, commit and file: when
`dynarmic` (Eden lineage) and ARMeilleure (Ryujinx) compile guest code (on the guest thread
or in the background), how they handle first execution, and how persistent caches are
loaded. No code is copied or paraphrased (LEGAL-20/21); the note replaces the RECALL in
ADR-0005.
- **Acceptance:** `docs/research/cpu-jit-scheduling.md` with evidence levels; ADR-0005's
  RECALL sentence updated to cite it.

### T-040 — Full A64 decode table and coverage tool
Populate the decoder's description table (T-033) to the v8.0–v8.5 feature set ADR-0004
requires (LSE, CAS/CASP, LDAPR, PAC, MTE, …), from Arm's public specification, and build
`lince-a64-coverage`, a Rust tool that decodes executables through the C API (ADR-0031) and
lists every encoding the table lacks. Decoding an encoding is not implementing it: the
interpreter only gains what the next phase's programs need.
- **Acceptance:** differential decode tests per instruction class; the coverage tool runs on
  the T-029 programs in CI and reports zero unknown encodings; it can be pointed at a
  user-supplied executable without any of its content entering the repository (LEGAL-50).
