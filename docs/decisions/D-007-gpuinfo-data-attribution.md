# D-007: May research notes use Vulkan Hardware Database (gpuinfo) data?

- **Raised by:** Claude Code (T-016) · **Date:** 2026-11-13
- **Blocks:** T-016
- **Status:** Answered (option A, 2026-11-13)

## Question
The T-016 survey (`docs/research/vulkan-feature-coverage.md`) transcribes device capability
values and coverage percentages from https://vulkan.gpuinfo.org. Its data is licensed under
Creative Commons Attribution 4.0 International, and Lince is GPL-3.0-or-later (ADR-0000). The
legal rules do not say whether third-party CC BY data may be kept in the repository or how to
credit it.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Use gpuinfo with an attribution block in every note that uses it | Broad coverage, including proprietary drivers on hardware the project does not own. CC BY 4.0 is listed by GNU as GPL-compatible. The material is small excerpts of facts, in documentation only. | Small | Yes |
| B Link to gpuinfo pages without transcribing values | No licence question. Notes become unreadable without the site, and pages change over time | Small | Yes |
| C Use only Lince's own `vulkaninfo` dumps, Mesa `features.txt` and vendor notes | Highest evidence level. Most proprietary-driver rows become UNKNOWN | Medium | Yes |

## Recommendation
Option A. Each note that uses gpuinfo data carries this block: creator and copyright notice as the
site shows it ("Vulkan Hardware Database © 2016-2026 by Sascha Willems"), a link to
https://vulkan.gpuinfo.org and every report used (full URLs, or a URL pattern plus report ids),
a link to
https://creativecommons.org/licenses/by/4.0/, and a statement that the tables are excerpts
transcribed by Lince. Lince's own `vulkaninfo` dumps can be added as VERIFIED data where hardware is
available.

## Consequences
- `docs/research/README.md` records the attribution rule for third-party CC BY data.
- If gpuinfo data is ever embedded in the shipped program (for example a generated per-driver
  override table), attribution must ship with the binary too; that needs a new decision.
