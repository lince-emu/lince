# D-010: May the SASS corpus be built with the CUDA toolkit, given its EULA?

- **Raised by:** Claude Code (`/next-task`, T-014) · **Date:** 2026-11-13
- **Blocks:** T-014; through it T-017, T-018 and T-023
- **Status:** Open

## Question
T-014 compiles team-written CUDA kernels with `nvcc` for `sm_86` and stores their SASS from
`cuobjdump`/`nvdisasm` (D-005). LEGAL-14 allows vendor tools "only as their licenses permit".
The CUDA Toolkit 13.5 EULA installed on the maintainer's machine
(`CUDA/v13.5/EULA.txt`, §1.2 "Limitations", item 8) says that the licensee may not reverse
engineer, decompile or disassemble any portion of the output generated using SDK elements
for the purpose of translating such output artifacts to target a non-NVIDIA platform.

The corpus exists to test Lince's SASS decoder and its SASS→SPIR-V translation (PoC-2,
PoC-4). That translation runs on AMD and other non-NVIDIA GPUs. Disassembling `nvcc` output
to build the corpus therefore appears to fall within that clause. The kernels are written by
the team, so this is not about third-party code (LEGAL-14's second half is met); it is about
the licence terms of the toolkit itself. Whether the clause applies is a legal question, which the
project rules reserve to the maintainer (LEGAL-52), so no agent should continue T-014 as
written.

Not affected by this question: SASS shipped inside Switch 2 titles is produced by the game
developers' toolchain, and Lince never runs NVIDIA's SDK on it.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Build the corpus without NVIDIA's SDK: the team writes Ampere SASS test programs with its own assembler, using encodings from public research (LEGAL-11), and computes expected outputs on the CPU as D-005 planned | No EULA question, since no SDK output is disassembled. Encodings come only from public sources, so coverage grows with the decoder (T-018) instead of arriving at once. There is no compiler-generated scheduling or control-flow shape, so that kind of realism must be added by hand | Medium: an assembler in `poc/` plus hand-written programs | Yes |
| B Pause T-014 until the maintainer has legal advice on §1.2.8, then proceed with D-005 as written if the advice allows it | Keeps the most realistic corpus (real `nvcc` output). Blocks T-014, T-017 and T-018 for an unknown time. Any later licence revision could reopen the question | Small for agents; legal cost for the maintainer | Yes |
| C Proceed with D-005 as written, reading §1.2.8 narrowly (for example, as covering only the translation of shipped applications) | Fastest. Relies on a reading the EULA text does not state, in a GPL project that will be public. A dispute would affect the corpus and every result built on it | Small | Hard: the corpus and its results would be committed and published |

## Recommendation
Option A. It removes the licence question instead of interpreting it, and it keeps PoC-2 and
PoC-4 moving under the existing doc 06 matching rule. The cost is a slower-growing and less
realistic corpus, which can be revisited if the maintainer later obtains legal advice
(option B). This is a legal question, so the maintainer decides; agents do not take T-014
until it is answered.

What happened before this was noticed: while checking the toolchain, the agent compiled one
three-line test kernel (an array addition) with `nvcc` and listed it with `nvdisasm` in its
temporary directory. Both files were deleted, and nothing was committed.

## Consequences
- **If A:** rewrite T-014's details and acceptance (own assembler, SASS programs from public
  encodings, CPU expected outputs), update D-005 and doc 06's PoC-2/PoC-4 corpus wording,
  and drop T-023 or reword it to running the team-assembled programs on NVIDIA hardware.
  Add a LEGAL rule stating that SDK output is not disassembled for translation work. The
  maintainer may also want to decide whether the CUDA toolkit stays installed.
- **If B:** T-014 stays `Needs decision` until the advice arrives.
- **If C:** record the reading and the accepted risk in `docs/09-legal-and-content.md`, then
  T-014 proceeds as written.
