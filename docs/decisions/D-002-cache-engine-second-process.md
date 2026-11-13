# D-002: Does an engine that cannot serve a second process qualify under ADR-0015?

- **Raised by:** Claude Code (T-010 review) · **Date:** 2026-11-13
- **Blocks:** the ADR-0015 verdict after PoC-6 measurements
- **Status:** Answered (option A, 2026-11-13, maintainer follows the recommendation)

## Question
`redb` holds an exclusive lock on its file, so a second process cannot open it, even
read-only. Doc 02 §5 step 1 requires a second emulator instance to fall back to read-only
access. The PoC-6 correctness list also includes a "concurrent reader process". ADR-0015's
revisit clause only compares performance ("within 20 % on every PoC-6 metric").

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Keep `redb` as a comparator, and add "serves a read-only second process" to the ADR-0015 adoption condition | The engine choice follows doc 02's requirements. `redb` still provides single-process performance data | Text change | Yes |
| B Drop `redb` from PoC-6 | Less work. Loses a pure-Rust performance reference | Small | Yes |
| C Ignore the requirement | Two instances would silently lose the cache, which contradicts doc 02 §5 | None | Yes |

## Recommendation
Option A. The requirement exists in doc 02, so the adoption rule should include it.
Keeping `redb` in the measurements is cheap and still tells us how far the custom store is
from a mature engine.

## Consequences
ADR-0015 "Conditions for revisiting" is amended in the same pull request as this record.
ADR-0015 stays Proposed.
