# D-003: How do frame-ledger events carry durations?

- **Raised by:** Claude Code (T-011 review) · **Date:** 2026-11-13
- **Blocks:** nothing now; the ledger work after PoC-7
- **Status:** Answered (option A, 2026-11-13, maintainer follows the recommendation)

## Question
Doc 05 §3 lists `CompileStart/End{tier}` and wait events without saying how their duration
is recorded. The PoC-7 analyzer attributes spikes by time covered, so it needs durations.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Duration in the event argument, `ts` = end of the interval | One event per interval. No start/end pairing, which ring overwrites can break. A wait still running at dump time is invisible | Doc edit | Yes |
| B Start and end event pairs | Shows intervals still in progress. Costs two events, and the analyzer must pair them across overwrites | Doc edit plus analyzer work | Yes |

## Recommendation
Option A for waits and blocking work (`PipelineWait`, `FenceWait`, `CacheResolve`,
`CacheWrite`, `CacheFsync`, `CompileEnd`). Keep `CompileStart` as its own event, so compiles
in progress stay visible in a dump. PoC-7 implements exactly this.

## Consequences
Doc 05 §3 is updated in the same pull request as this record.
