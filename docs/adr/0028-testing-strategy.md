# ADR-0028: Testing strategy

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** doc 05 §8; doc 09 §5; ADR-0009, ADR-0010

## Context
Correctness bugs in shader translation look like rendering glitches far from their cause
(Lichii rows 93, 95). Performance must be tested as a correctness property.

## Requirements
Fast unit tests; differential tests between execution tiers; deterministic performance tests
without shipping game content.

## Alternatives
1. Game-based manual testing.
2. **Layered automated testing:** unit/property → differential → capture replay → scripted
   gameplay scenarios.

## Decision
Option 2:
- **Unit/property:** decoders, IR passes, cache store (with fault injection), semaphore model.
- **Differential:** A64 interpreter vs. JIT tiers; shader Tier 0 vs. Tier 1 vs. Tier 2 on the
  same inputs; CPU reference ASTC decoder vs. GPU decoder.
- **Fuzzing:** A64 and SASS decoders, cache reader, IPC parsers (`cargo fuzz`).
- **Replay:** recorded GPU command streams on private CI runners for performance and image
  regression.
- **Scenarios:** the 20 scripted scenarios of doc 05 §6 on reference hosts.

## Technical justification
Differential testing turns multiple tiers from a risk into an oracle.

## Advantages
Most bugs caught without running games.

## Disadvantages
Capture infrastructure needed early.

## Risks
Private corpora limit outside contributors. Mitigation: public synthetic corpora (own CUDA SASS,
synthetic command streams).

## Validation plan
Coverage of opcode tables by differential tests reported in CI.

## Conditions for revisiting
None.
