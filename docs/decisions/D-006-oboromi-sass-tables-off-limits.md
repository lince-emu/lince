# D-006: Treat oboromi's SASS decoder tables as off-limits for Lince?

- **Raised by:** T-015 research session · **Date:** 2026-11-13
- **Blocks:** T-018 (PoC-4 SASS decoder)
- **Status:** Open

## Question
oboromi (github.com/0xNikilite/oboromi @ `c97664a8460e`) generates its Ampere SASS decoder
from `scripts/sm_86_instructions.txt` and `scripts/sm_86_latencies.txt`, and ships
`docs/sm86.pdf`. The repository gives no provenance for these files, and the text table's
format looks like a machine-description table from NVIDIA's toolchain. If it was extracted
from vendor binaries, using it, or anything generated from it, would conflict with LEGAL-10
and LEGAL-14. No Accepted ADR or legal rule names this source, so the maintainer must decide
whether anyone working on T-018/PoC-4 may consult it. See
`docs/research/switch2-emulator-projects.md` §5 L1.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended): off-limits — the tables, the PDF, the generated decoder (`sm86_decoder_generated.rs`, `sm86.rs`) and anything derived from them | Removes the provenance risk; T-018 relies only on the T-014 corpus and public documentation | None now; slower decoder coverage later | Yes, if provenance is later shown to be clean |
| B: allowed after the maintainer verifies provenance with the oboromi authors | Could speed up encoding coverage | Maintainer time; outcome uncertain | Hard: knowledge cannot be un-read |
| C: allowed now | Fastest | Exposes the project to LEGAL-10/14 risk | No |

## Recommendation
Option A. Lince's SASS encoding knowledge should come from its own corpus (T-014, D-005) and
public sources. The research session stopped reading the table after identifying its format
and did not open the PDF; keeping it that way costs nothing now.

## Consequences
Add oboromi's SASS tables and generated decoder to the list of forbidden sources in
`docs/conventions/legal-rules.md` (or an ADR-0029-style note) once answered; T-018 can proceed
under option A without further changes.
