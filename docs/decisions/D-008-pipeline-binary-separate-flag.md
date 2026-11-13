# D-008: Should `VK_KHR_pipeline_binary` be a V3 requirement or a separate flag?

- **Raised by:** Claude Code (T-016) · **Date:** 2026-11-13
- **Blocks:** ADR-0007 and ADR-0013 acceptance (tier table)
- **Status:** Answered (option A, 2026-11-13)

## Question
ADR-0007 lists pipeline binary among the V3 requirements. The T-016 survey
(`docs/research/vulkan-feature-coverage.md` §3, §8) shows Intel's Windows driver (Arc B580,
101.9175, report dated 2026-11-05) with GPL + fast link, fragment barycentrics, subgroup size
control and mesh shaders, but without shader objects and pipeline binary. The same report also
lists `VK_EXT_extended_dynamic_state3`, `VK_EXT_descriptor_buffer`, `VK_EXT_descriptor_indexing`
and `VK_KHR_buffer_device_address` (checked by the invariant reviewer, 2026-11-13). Under the
current table it lands in V2 only because of pipeline binary, which serves cache persistence (L6,
ADR-0013), not the pipeline-creation model the tiers describe.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Remove pipeline binary from V3 and make it a separate flag, like mesh shaders | Tiers describe only the pipeline model and Tier 0 coverage. Hosts without it use the sharded `VkPipelineCache` L6 path that V2 already needs. One more combination to test (V3 with and without the flag) | Small (ADR text) | Yes |
| B Keep it in V3 | Simpler tier definition. Intel on Windows loses full Tier 0 for a persistence feature | None | Yes |

## Recommendation
Option A. The `VkPipelineCache` fallback must exist for V2 anyway, so the extra test combination
is cheap, and the tier no longer depends on a feature that only affects warm-start cost.

## Consequences
- ADR-0007 (Proposed): pipeline binary moves from the V3 row to the separate-flag sentence.
- ADR-0013 (Proposed): the V3 bullet uses pipeline binary for L6 when the flag is present and the
  sharded `VkPipelineCache` otherwise.
- T-022 (pipeline binary PoC) is unchanged.
