# ADR-0024: HLE architecture

- **Status:** Proposed
- **Date:** 2026-11-13
- **Related:** doc 08 §3; doc 09; ADR-0017

## Context
No firmware is allowed; all kernel and service behavior is original HLE. Switch 2 adds many
services and commands, several still partly guessed by other projects.

## Requirements
Original implementations; IPC protocols (CMIF, TIPC) exact; unknown commands visible, never
silently "successful" by default; services must not block guest cores unnecessarily.

## Alternatives
1. Auto-stub unknown commands with success (fast bring-up, hides bugs).
2. **Explicit service registry**: typed command handlers generated from declarative service
   descriptions; unknown commands return a distinct error and a ledger event; a debug-only
   auto-stub mode for bring-up.
3. LLE of system modules (requires firmware; rejected).

## Decision
Option 2. Kernel objects (threads, events, mutex/condvar via address arbitration, memory) in
`lince-kernel`; services in `lince-hle` / `lince-hle-nx2`, each with protocol tests. Blocking
service calls run on service host threads and resume the guest thread through kernel events.

## Technical justification
Declarative descriptions make protocol differences between Switch 1 and 2 explicit and testable;
avoiding default auto-stubs prevents silent wrong behavior (Lichii needed auto-stubbing for
bring-up and then fixed many replies, e.g. TIPC reply layout bug in row 22).

## Advantages
Traceable bring-up; fewer silent failures.

## Disadvantages
Slower first boot of a new title than blanket auto-stubbing.

## Risks
Guessing protocols. Mitigation: every guessed field documented as such in `docs/research/`.

## Validation plan
Protocol unit tests per command; boot traces per title listing unknown commands.

## Conditions for revisiting
None foreseen.
