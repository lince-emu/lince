# D-004: What concurrency evidence does the ledger ring need before ADR-0027 is accepted?

- **Raised by:** Claude Code (T-011 review) · **Date:** 2026-11-13
- **Blocks:** ADR-0027 acceptance
- **Status:** Answered (option A, 2026-11-13, maintainer follows the recommendation)

## Question
The PoC-7 ring is a fence-based seqlock. Its memory ordering has been checked only by
argument and by a stress test on x86-64. x86-64 has strong memory ordering, so that test
cannot catch a missing or weakened fence.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) A `loom` model of push/snapshot, plus a test run on ARM | `loom` (MIT) as a dev-dependency only, with no release impact. ARM has a weak memory model | Small | Yes |
| B ARM run only | No model. Catches only the bugs the stress test happens to hit | Smaller | Yes |
| C Argument only | No evidence of memory-model correctness | None | Yes |

## Recommendation
Option A. The protocol is subtle, and `loom` is the standard tool for exactly this case. An
ARM run adds real weak-memory hardware. Neither affects release builds.

## Consequences
New task T-021: a `loom` model of the PoC-7 ring and a test run on an ARM host.
