# D-011: Which public source may T-014 use for Ampere SASS encodings?

- **Raised by:** Claude Code (`/next-task`, T-014) · **Date:** 2026-11-13
- **Blocks:** T-014; through it T-017 and T-018
- **Status:** Answered (option B, Mesa NAK, 2026-11-13, maintainer in chat)

## Question
LEGAL-15 forbids the project from disassembling NVIDIA SDK output, so T-014 needs Ampere
encodings from a public source (LEGAL-11). Every public source found was itself derived from
NVIDIA tools: Mesa's NAK compiler tests its encodings against `nvdisasm`; nvopen-tools
reverse-engineers `ptxas` (forbidden outright by EULA §1.2 item 1); CuAssembler-style tables
come from `nvdisasm` runs. NVIDIA's public *CUDA Binary Utilities* lists mnemonics only.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A Corpus at the mnemonic level only, encodings deferred | No provenance question now; T-018 stays blocked on bits | Small | Yes |
| B Mesa NAK (MIT) as the encoding source | Open-source driver compiler shipped by Linux distributions; its authors validated against `nvdisasm` under their own EULA, not ours | Medium | Yes |
| C Stop T-014 for legal advice | Blocks T-014, T-017, T-018 | None | Yes |

## Answer
Option B. Encoding facts may come from Mesa's NAK (`src/nouveau/compiler/nak/`, MIT),
pinned to a commit and cited per field in `docs/research/ampere-sass-encoding.md`. NAK code
is not copied or translated (LEGAL-20/21/22); only the bit positions and opcode values it
documents are used. nvopen-tools and `nvdisasm`-derived tables remain off-limits, as do
oboromi's tables until D-006 is answered.
