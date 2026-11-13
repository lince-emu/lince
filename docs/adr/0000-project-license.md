# ADR-0000: Project license

- **Status:** Proposed — requires the project owner's decision before the first code commit
- **Date:** 2026-11-13
- **Related:** doc 09, ADR-0026

## Context
The license decides what contributors may reuse, how forks behave, and how strict the
clean-room process must be. The main reference projects are GPL (yuzu lineage, Dolphin),
MPL-2.0 (Cemu) and MIT (Ryujinx). Lichii's README warns about malicious third-party builds,
a reminder that forks and redistribution are a real concern.

## Requirements
- Compatible with the Rust crates we intend to use (mostly MIT/Apache-2.0).
- Clear policy for code reuse from other emulators.
- Decided before any code exists.

## Alternatives
1. **GPL-3.0-or-later** — forks must stay open; compatible with consuming MIT/Apache crates;
   would allow (with attribution) reuse of GPLv3-compatible code from GPL emulators.
2. **MPL-2.0** — file-level copyleft; easy embedding; compatible with Cemu-style sharing.
3. **MIT OR Apache-2.0** — maximal reuse by others; no reuse of GPL code possible; closed forks allowed.

## Decision
Recommended: **GPL-3.0-or-later**, pending owner confirmation. Independent of the choice, the
clean-room rules of doc 09 apply (Lince-NX is designed from scratch, not assembled from others'
code).

## Technical justification
GPL keeps derivative builds open and aligns with most of the ecosystem; it does not restrict
our use of permissive Rust crates.

## Advantages
Protects against closed forks; ecosystem alignment.

## Disadvantages
Some downstream users (e.g. companies embedding components) are deterred.

## Risks
A later license change requires every contributor's consent.

## Validation plan
Owner decision; license headers checked in CI (`xtask ci`).

## Conditions for revisiting
Before the first public release only.
