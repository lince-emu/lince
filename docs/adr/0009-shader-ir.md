# ADR-0009: Ampere-native shader IR with explicit dependencies

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** doc 01 E5, E6, E9; doc 03 §7; ADR-0010, ADR-0012, ADR-0025

## Context
Eglantine/Lichii lower Ampere SASS to Maxwell instructions and reuse yuzu's Maxwell recompiler.
Any instruction the lowering cannot express rejects the whole program and its draws are skipped
(doc 01 E5). Translation also reads live GPU state, which kept it on the GPU thread (E9).

## Requirements
Express Ampere semantics directly (uniform datapath, convergence barriers, predicates, new
memory and warp ops, tensor ops); allow translation on worker threads; serve Tier 0 pre-decode,
Tier 1 and Tier 2; later accept Maxwell SASS (Switch 1).

## Alternatives
1. Lower Ampere to Maxwell and use a Maxwell-shaped IR.
2. Translate SASS directly to SPIR-V.
3. Use an existing compiler IR (SPIR-V itself, naga IR, Cranelift IR, LLVM IR).
4. **Own SSA IR designed for NVIDIA SASS semantics**, with an explicit dependency model.

## Decision
Option 4.
- SSA with first-class predicates, uniform registers/predicates, carry flags, and convergence
  tokens for `BSSY/BSYNC/WARPSYNC`.
- **Dependency model:** every read of guest state outside the program (constant-buffer values
  used structurally, texture type/format of a handle, viewport transform mode, etc.) goes
  through a `DependencyRecorder`. The recorded set is stored (cache L1) and defines the variant
  key. Translation consumes an immutable snapshot of those values, never live state.
- Unsupported instructions become an explicit `Unimplemented` IR op with the raw bits, so a
  program still translates; the op traps into a well-defined debug path and is counted. A
  program with such an op is excluded from Tier 1/2 and stays on Tier 0 if Tier 0 supports it.
- Structurization to structured control flow is an IR pass, not part of decoding.
- The IR is independent of the host shader language. SPIR-V (Vulkan) is the first emitter;
  DXIL or HLSL (Direct3D 12) and MSL (Metal) emitters are planned later, alongside the
  SPIR-V emitter, not as translations of its output. Target-specific lowering happens in each
  emitter.

## Technical justification
Options 1 and 2 recreate the whole-program rejection failure; 3 would bend our semantics to a
foreign model (SPIR-V's structured CFG is a target, not a working IR).

## Advantages
Precise semantics; worker-thread translation; one IR for both Switch generations.

## Disadvantages
A compiler to build and maintain.

## Risks
Structurizing arbitrary SASS control flow (jump tables, calls, library `EXIT`s) is hard.
Mitigation: reuse well-known algorithms (e.g. relooper-style structurization) and fuzz.

## Validation plan
PoC-4: decode + IR + SPIR-V for a corpus of team-assembled SASS programs (T-014, LEGAL-15)
and privately dumped graphics programs; differential testing per opcode under the doc 06
matching rule.

## Conditions for revisiting
Never for option 1. Revisit internals if translation P99 latency per instruction blocks Tier 1
turnaround.
