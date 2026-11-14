# ADR-0024: HLE architecture

- **Status:** Proposed. B1 (doc 07) needs only the part marked **B1 scope** below accepted.
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
- **SVC boundary:** SVCs are dispatched through a table indexed by number, including the
  Switch 2 SVC `0x80` (doc 01 E2). An unknown SVC or command returns a distinct error and is
  logged once by name and number (ADR-0027); it never reports success by default.
- **Names:** service, command and structure names come from public documentation, otherwise
  descriptive names chosen by the project; never names recovered from game symbols
  (LEGAL-12; `docs/research/switch2-emulator-projects.md` L2).

**B1 scope:** the SVC boundary above, for the memory, exit and `OutputDebugString` SVCs.

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
