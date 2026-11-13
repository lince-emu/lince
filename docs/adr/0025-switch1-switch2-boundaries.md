# ADR-0025: Switch 1 / Switch 2 component boundaries

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** charter §2.2; ADR-0004, ADR-0009, ADR-0024

## Context
Switch 2 is first; Switch 1 support comes later without compromising Switch 2 design.

## Requirements
Share only where commonality is demonstrated; isolate generation-specific behavior.

## Alternatives
1. Generic abstractions for both from day one.
2. **Switch 2 only now, with seams placed where commonality is already known.**
3. Separate emulators.

## Decision
Option 2. Known-shared: A64 decoder/JIT (Switch 1 adds AArch32 as a separate crate), kernel,
IPC framework, filesystem formats, shader IR, Vulkan backend, cache, scheduler, trace. Known
separate: GPU classes (Maxwell vs Ampere), macro engines (MME vs MME64), SASS front-ends, NVN vs
NVN2 services, memory layouts. Seams: SASS front-end → IR; class engines → packet format;
service registry per system generation.

## Technical justification
These seams already exist in the architecture for other reasons, so Switch 1 costs nothing now.

## Advantages
No premature abstraction; clear plug-in points later.

## Disadvantages
Some duplication when Switch 1 arrives.

## Risks
Accidental Switch 2 assumptions in shared crates. Mitigation: review checklist item.

## Validation plan
At Switch 1 kickoff, review shared crates for generation-specific code.

## Conditions for revisiting
Start of Switch 1 work (M13).
