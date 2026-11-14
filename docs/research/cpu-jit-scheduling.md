# Research: how dynarmic and ARMeilleure schedule guest-code compilation (T-039)

## 1. Question

When do dynarmic (Eden lineage) and ARMeilleure (Ryujinx lineage) compile guest code: on the
guest thread or in the background? What happens on first execution (interpret, compile-then-run,
tiers)? How is the persistent cache (PPTC) loaded: eagerly at boot or lazily?

Pinned sources (cloned outside the Lince tree, sparse, in `../research/`):

- **Eden:** `eden-emu/eden` (`https://git.eden-emu.dev/eden-emu/eden.git`), branch `master`, commit
  `63e957b1cad697558beb782b0947f01227efc826`. dynarmic is vendored under `src/dynarmic/`.
- **Ryujinx lineage:** the original `Ryujinx/Ryujinx` repository is no longer public. Read instead
  the maintained fork Ryubing, `https://git.ryujinx.app/ryubing/ryujinx.git`, branch `master`, commit
  `4d5a19d3311b0980fe91ba17dcce9033f57fd280` (`src/ARMeilleure/`). It is a fork, so
  "ARMeilleure" below means this fork's copy; upstream-era behavior is not independently verified.

## 2. Short answer

- **dynarmic compiles on the guest thread, synchronously, with no tiers.** The first execution of
  any address translates, optimizes and emits one block, then runs it. There is no interpreter
  fallback and no background compile worker in the CPU path. A cache-full condition flushes the
  whole code cache on the same thread.
- **ARMeilleure also compiles first execution on the guest thread** (low-quality tier, no
  interpreter). After a function has been called 100 times, a counter in the emitted code queues
  a high-quality recompile, done by 1 to 4 background threads that swap the result in.
- **PPTC loads eagerly, before the first guest instruction.** The whole file is decompressed into
  memory, entries are validated by hashing guest code, and any profiled function missing from the
  cache is recompiled in a blocking, multi-threaded step before the guest starts. Only afterwards
  do the background rejit threads start.
- Neither project does what ADR-0005 proposes (interpret until a worker publishes). Their first-run
  stutter model is "compile on the guest thread"; ARMeilleure's PPTC is the mitigation, and its
  cost is boot time proportional to the profile.

## 3. Findings

Paths are relative to each repository root. "Lince?" says whether the behavior bears on a Lince
decision (Y = informs, N = contrast only).

| # | Source (repo@commit:file) | Observation (own words) | Evidence | Lince? |
|---|---|---|---|---|
| F1 | eden@63e957b:`src/dynarmic/src/dynarmic/backend/x64/a64_interface.cpp` (block lookup, ~l.244-263) | Looking up a block for a guest location first checks the emitter's table. On a miss, the same call translates guest instructions to IR, runs the optimization passes, and emits host code, then returns the entry point. All on the calling thread. | SRC | Y: confirms the "compile on the guest thread" stutter source that ADR-0005 removes |
| F2 | same file, ~l.248-254 | Before compiling, if less than 1 MiB of code space remains, the entire code cache and emitter tables are dropped immediately, on the guest thread. Execution then recompiles blocks on demand. | SRC | Y: cache-full is a stall source; Lince Tier B/C need bounded, background eviction |
| F3 | eden@63e957b:`.../backend/x64/block_of_code.cpp` (dispatcher, ~l.299-324) | The dispatcher stub that runs between blocks checks the halt flags (and the cycle budget when counting is on), then calls the host's block-lookup callback. So an unlinked edge returns to the dispatcher, which compiles if needed. | SRC | Y |
| F4 | eden@63e957b:`.../a64_emit_x64.cpp` (terminal emission, ~l.620-660) | With block linking enabled, a direct branch is emitted as a patchable jump: if the successor already exists it is wired directly, otherwise it goes to the dispatcher and is patched later. Linking is disabled in single-step mode. | SRC | Y: patching needs the synchronization ADR-0005 lists as a risk |
| F5 | eden@63e957b:`.../a64_interface.cpp` (`Run`/`Step`, invalidation, ~l.74-116, 265-285) | Cache invalidation requests (range or full) are recorded under a mutex and signalled by a halt flag; the actual invalidation happens on the executing thread at the next entry or exit of `Run`. | SRC | Y: invalidation is cooperative with the guest thread |
| F6 | eden@63e957b:`src/core/arm/dynarmic/arm_dynarmic_64.cpp` (`RunThread`/`StepThread`, ~l.351-359) | Eden's CPU wrapper runs the guest by calling the JIT's run entry directly from the kernel-thread runner. No compile queue or precompile pass at this layer. | SRC | N |
| F7 | eden@63e957b: search of `src/dynarmic/src` and `src/core/arm` for persistence/serialization/disk cache | No persistent translation cache found. The only host `std::thread` use under dynarmic is the macOS exception handler. Negative result of a text search, not a proof of absence. | SRC (negative) | Y: dynarmic offers neither a disk cache nor background compilation to compare against |
| F8 | eden@63e957b:`docs/dynarmic/Design.md` | Design doc describes translate to IR, optimize, emit, with block linking and a return-stack-buffer prediction; consistent with F1-F4. | DOC | N |
| F9 | ryubing@4d5a19d:`src/ARMeilleure/Translation/Translator.cs` (`GetOrTranslate`, ~l.215-238) | First call of an unknown address translates it right there, in the low-quality tier, publishes it in the function table and registers it with the profiler. No interpreter. | SRC | Y |
| F10 | same file, `EmitRejitCheck` (~l.484-503) | Low-quality code carries a per-function call counter; when it reaches exactly 100, the code calls out to enqueue a high-quality recompile of that function. High-quality code carries no counter. | SRC | Y: hot threshold is a fixed small call count, not time-based |
| F11 | same file, `Execute` (~l.94-119) and `BackgroundTranslate` (~l.320-339); `TranslatorQueue.cs` | Background rejit threads: count is `max(1, (cores-6)/3)` capped at 4; the last one (when more than one) runs at lowest OS priority. They pop requests from a LIFO stack (most recent hot function first, de-duplicated by address), recompile at high quality, and replace the old entry; the old code is queued for later deletion. Started only after the PPTC step completes. | SRC | Y: worker count and LIFO ordering are heuristics to measure, not copy |
| F12 | same file, `Execute` (~l.81-93) | On the first guest thread to enter, if PPTC is enabled: load cached translations, then generate the missing ones (F14), then start the rejit threads. Guest execution of that thread begins only after both. | SRC | Y: boot cost grows with cache; ADR-0005/§6 expectation confirmed |
| F13 | ryubing@4d5a19d:`.../Translation/PTC/Ptc.cs` (`Initialize`, `PreLoad`, `Load`, ~l.106-330) | The cache file is decompressed in full into an unmanaged buffer; outer and inner headers are checked (version, host architecture, debugger mode) and each section is verified by a 128-bit hash. Primary file falls back to a backup file. Any mismatch discards the whole cache. | SRC | Y: whole-file eager load and all-or-nothing validation violate Lince invariant 4 by design |
| F14 | same file, `LoadTranslations` (~l.584-667) | Per entry: the hash of the current guest bytes is compared with the stored hash; a mismatch (or a profile now saying the function should be high quality) turns the entry into a stub and the function is recompiled later. Valid entries have their relocations patched and are registered in the function tables and function map. Relocation symbols stand in for addresses (function table base, call counters, etc.). | SRC | Y: validate-by-guest-content-hash plus relocatable code matches Lince's snapshot-key idea |
| F15 | same file, `MakeAndSaveTranslations` (~l.841-957) | Functions named by the profile but absent from the cache are translated before the game starts, in parallel: thread count equals logical cores, one less if more than 4, a third in low-power mode. A progress event is reported while it runs. Then a low-priority background thread writes the updated cache to disk. | SRC | Y |
| F16 | ryubing@4d5a19d:`.../PTC/PtcProfiler.cs` (~l.59-125, 125-138) | The profile records only addresses inside the main program's static code range, with execution mode and quality flag; dynamically generated or later-loaded code is not profiled. Blacklisted functions (for example ones that decode as undefined at entry) are skipped. | SRC | Y: JIT-ted guest code is not cacheable by address; Lince cache keys must say so |
| F17 | ryubing@4d5a19d:`src/ARMeilleure/Translation/Translator.cs` `Translate` (~l.248-318) | When PPTC is active, emitted code is compiled in relocatable form and handed to the cache writer with its hash and quality flag. | SRC | Y |

## 4. Open questions

- Ryubing is a fork; upstream Ryujinx behavior at its last public commit is not verified here.
  Constants (100 calls, thread formula) may differ upstream. Needs a MIRROR or archived tag if the
  numbers are ever relied on. (They should not be: Lince measures its own.)
- Ryubing also contains `Ryujinx.Cpu/LightningJit` (a second ARM64-host JIT) and an Apple
  Hypervisor path. Their scheduling was not read.
- dynarmic's ARM64 backend (`backend/arm64`) was not read; x64 backend only.
- Eden's native-code-execution (NCE) path on ARM64 hosts was not read; it bypasses dynarmic.
- No stutter was measured; "first-run stall" is inferred from F1/F9 (compile on the calling thread),
  not timed (GUESS on magnitude).

## 5. Affected ADRs, PoCs and tasks

- **ADR-0005** (interpreter and JIT): supports its premise (the compared projects compile on the
  guest thread) and its tier design is a deviation, not a copy. Replace the RECALL sentence
  (below). Possibly add one line: Lince's cache must stay lazy, unlike F12/F13.
- **R3 in `docs/01` (CPU cache requirement):** F12-F16 turn R3 from MIRROR+RECALL into
  SRC for the PPTC part; "background translation of new code" is accurate only for the rejit
  tier, not for first execution.
- **T-038 (LCIR v1)** and the first JIT tier: use F5 and F4 as the list of synchronization hazards
  (invalidation, patching, whole-cache eviction).
- **Lazy cache lookup (JIT cache design):** F13-F15 are the anti-pattern list (eager whole-file load,
  blocking pre-translation).

### ADR-0005 sentence currently RECALL

File `docs/adr/0005-interpreter-and-jit.md`, section "Technical justification", lines 103-105:

> How `dynarmic` and ARMeilleure schedule compilation is not yet documented with sources (RECALL
> only); a research note should confirm it before the JIT tiers are implemented.

Suggested replacement (behavior only, with citation):

> `dynarmic` (Eden `63e957b`) compiles every first-executed block on the guest thread with no
> interpreter or tiers, and ARMeilleure (Ryubing fork `4d5a19d`) does the same for its low-quality
> tier, moving only hot-function recompilation (100 calls) to 1 to 4 background threads; PPTC is
> loaded and its gaps recompiled before the guest starts. See `docs/research/cpu-jit-scheduling.md`.
