# Pluggable JIT backends behind LCIR

- **Discarded:** 2026-11-14, by the maintainer, in chat.
- **Related:** ADR-0005 (interpreter and JIT), ADR-0030, ADR-0031.

## The idea
Make the CPU JIT backend replaceable: `A64 decoder → LCIR → backend → code cache +
dispatcher`, with Lince's own backend by default and LLVM or Cranelift as alternative
backends behind the same interface, so switching frameworks later would not touch the front
end, the IR, the cache or the dispatcher.

## Why it was discarded
- **Speculative abstraction.** Only one backend is planned; building a plug-in layer for
  backends that may never exist is what AGENTS.md §3 rules out.
- **Lowest common denominator.** Every backend would have to produce Lince's relocatable code
  format, patchable entry points and the precise host-PC → guest-PC fault metadata ADR-0031
  needs. General-purpose frameworks make the last one hard, and an interface wide enough for
  them would constrain the own backend.
- **Unbounded compile time.** A general-purpose optimizer's compile time is not bounded by
  Lince, which works against the stutter goal even when run in the background.
- **The useful part is kept.** LCIR stays a clean, plain-data contract (ADR-0005), so a
  different backend is still possible later without designing for it now.

## What would reopen it
Measurements showing the own Tier C backend cannot reach the performance a title needs, with
a specific framework shown (in a PoC) to close the gap within the compile-time budget.
