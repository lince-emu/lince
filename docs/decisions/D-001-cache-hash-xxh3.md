# D-001: Which hash implementation do the cache keys and payload checksums use?

- **Raised by:** Claude Code (T-010 review) · **Date:** 2026-11-13
- **Blocks:** T-020
- **Status:** Answered (option A, 2026-11-13, maintainer follows the recommendation)

## Question
Doc 02 §3 and §4.1 specify XXH3: 128-bit keys and a 64-bit payload checksum. PoC-6 had no
approved hash dependency, so it used CRC-32C for payloads. Its cost compared with XXH3-64 is
an unmeasured factor in the PoC-6 resolve latency.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) `xxhash-rust` with the `xxh3` feature | Pure Rust. Provides both XXH3-64 and XXH3-128. Licence BSL-1.0, already on the `deny.toml` allow-list | Small | Yes |
| B `twox-hash` 2.x | Pure Rust, MIT. Fewer users of its XXH3 path | Small | Yes |
| C Keep CRC-32C | No dependency. Diverges from doc 02, and CRC-32C cannot provide the 128-bit keys | None | Yes |

## Recommendation
Option A. `lince-cache` needs XXH3-128 for keys anyway, so deciding once avoids two hash
dependencies. The licence is already allowed. PoC-6 should first measure CRC-32C and XXH3-64
side by side, so the payload-checksum choice rests on data.

## Consequences
New task T-020: add `xxhash-rust` to `poc/cache-store` and time both checksums in the
harness. Doc 02 is unchanged.
