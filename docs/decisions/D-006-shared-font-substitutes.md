# D-006: Shared-font substitutes — OFL-1.1 fonts, button glyphs and the Switch 1 font container

- **Raised by:** T-019 research session · **Date:** 2026-11-13
- **Blocks:** shared-font implementation in `pl:u` (no task yet; follows T-019)
- **Status:** Open

## Question
Games read the shared system fonts through `pl:u` (research note
[`system-data-and-applets.md`](../research/system-data-and-applets.md) F5, F6). Lince must ship
substitutes (LEGAL-06). Three points are not settled by the existing rules:

1. **Licensing.** The candidates (BIZ UDPGothic, Noto Sans SC/TC/KR) are SIL OFL-1.1. OFL-1.1
   allows bundling with any software when the fonts stay separate files shipped with their
   license (LEGAL-34). Subsetting to fit the 0x1100000-byte shared memory counts as modifying
   the font under OFL-1.1: if a font declares Reserved Font Names, the subset must be renamed,
   and `OFL.txt` plus any RFN notice must ship next to it. The project rules say to ask the
   maintainer when a license question is in doubt (legal-rules preamble; LEGAL-32 governs
   licenses only for Rust dependencies).
2. **Button and icon glyphs.** Lince draws its own glyphs at the private-use codepoints that
   games use for controller prompts. Some of those glyphs on the console depict Nintendo logos,
   marks or hardware. LEGAL-40 forbids Nintendo logos and trade dress in the UI. Proposed: such
   glyphs are neutral generic shapes (for example a plain circled letter) or blank.
3. **Switch 1 font container.** Titles that use the Switch 1 `pl:u` commands expect each font in
   shared memory wrapped in an 8-byte header that XOR-transforms the data with a fixed constant
   (SRC: Lichii@`0da698bb98c619e7eaec7dd2db80173c5134c255`:`src/core/hle/service/ns/platform_service_manager.cpp`,
   which calls it a key and encryption). Switch 2 commands 2000/2010 use plain TTF (note F5).
   The constant is public and Lince would apply it only to fonts it ships itself, so nothing
   protected would be decrypted — but the step is named as encryption, so it needs a ruling
   under LEGAL-02 and LEGAL-05.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Ship OFL-1.1 fonts (renamed subsets where an RFN applies) with `OFL.txt` in `assets/system/fonts/`; generic glyphs for logo-like icons; serve only the Switch 2 plain-TTF path until point 3 is ruled on | No transform code; titles that only use the Switch 1 commands get no fonts until then | Low | Yes |
| B As A, and also generate the Switch 1 container with its public constant | Text renders on both command paths; needs the maintainer (and counsel, before release) to accept point 3 | Low | Yes |
| C No bundled fonts; report the font load as unavailable | No licensing question; most titles show no text or fail | None | Yes |

## Recommendation
Option A now. It settles points 1 and 2 with no legal risk and keeps every Switch 2 title that
uses commands 2000/2010 working. Move to Option B only if the maintainer rules that the Switch 1
container is a format transform, not circumvention; which titles still use the Switch 1
commands is not yet known.

## Consequences
A task for the font PoC (subset, rename where needed, fit into the 0x1100000-byte shared
memory, measure metric differences) and an `assets/system/` layout with license files. doc 10
§3 links this decision.
