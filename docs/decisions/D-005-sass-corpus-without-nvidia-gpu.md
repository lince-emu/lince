# D-005: How is the SASS corpus built when the maintainer has no NVIDIA GPU?

- **Raised by:** Claude Code (backlog review) · **Date:** 2026-11-13
- **Blocks:** T-014, T-017, T-018
- **Status:** Answered (option A, 2026-11-13)

## Question
T-014 assumed the CUDA toolkit and an NVIDIA GPU on the maintainer's machine. The
maintainer's machine has an AMD GPU. Testing on NVIDIA hardware needs help from someone else.
Two parts of T-014 have different requirements. Compiling the kernels to SASS and
disassembling them (`nvcc`, `cuobjdump`, `nvdisasm`) works without an NVIDIA GPU. Running the
kernels to record reference outputs needs one.

## Options
| Option | Trade-offs | Effort | Reversible? |
|--------|-----------|--------|-------------|
| A (recommended) Split the task: compile the corpus locally (T-014), and record reference outputs on a borrowed NVIDIA GPU with a one-command script (T-023) | T-017 and T-018 can start now. T-017 checks against expected values computed on the CPU until T-023 delivers. | Small | Yes |
| B Wait until someone with an NVIDIA GPU does all of T-014 | Nothing to change. T-017 and T-018 wait for an unknown date | None | Yes |
| C Rent a cloud GPU | No dependency on other people. It costs money and needs an account | Small | Yes |

## Recommendation
Option A. The SASS is all that the T-018 decoder needs, and the kernels are written by the
team, so their expected results can be computed on the CPU. Only the final bit-exact check of
PoC-2 and PoC-4 needs real hardware. That check runs once, from a script anyone can execute.

## Consequences
T-014 becomes an agent task that compiles the corpus. The maintainer installs the CUDA
toolkit, without the driver. The new task T-023 records the reference outputs on an
NVIDIA GPU of compute capability 8.6 to 8.9 (GeForce RTX 30 or 40 series). An `sm_86` cubin
runs only on devices with compute capability 8.x where x ≥ 6. T-017 and T-018 depend on
T-014 only. The PoC-2 and PoC-4 reports cannot claim bit-exact results until T-023 is done.
