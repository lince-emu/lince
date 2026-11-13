# SASS corpus (T-014)

A team-written corpus of Ampere (SM 8.x) SASS programs for the shader decoder and the Tier-0
interpreter (PoC-4, PoC-2), built without any NVIDIA SDK tool (LEGAL-15, decisions
[D-005](../../docs/decisions/D-005-sass-corpus-without-nvidia-gpu.md),
[D-010](../../docs/decisions/D-010-cuda-eula-sass-corpus.md),
[D-011](../../docs/decisions/D-011-sass-encoding-source.md)).

| Path | Content |
|------|---------|
| `programs/*.sass` | Corpus programs in text form, each with its purpose in its header comment |
| `out/<name>.bin` | The program encoded: 16 bytes per instruction, little-endian words |
| `out/<name>.expected` | Expected final memory, `0xADDRESS 0xVALUE` per word, from the CPU reference |
| `src/isa.rs` | Encoder; every field from [`ampere-sass-encoding.md`](../../docs/research/ampere-sass-encoding.md) |
| `src/parse.rs`, `src/exec.rs` | Text parser; CPU reference execution (one thread) |

## Running

```bash
cargo test
cargo run --bin corpus -- build     # rewrite out/ from programs/
cargo run --bin corpus -- check     # fail if out/ is stale
cargo run --bin corpus -- run-gpu   # T-023 runner: stops (exit 2) where it needs an NVIDIA GPU
```

## Text form

One instruction per line, `#` comments, `name:` labels, `@P0`/`@!P0` guards and optional
scheduling control after `;` (`d=` stall 0–15, `wr=`/`rd=` scoreboard 0–5, `wait=` mask,
`yield`; default: stall 15, no scoreboards). `.word ADDR V…` sets initial memory. Global
addresses are 64-bit register pairs (`R4` low, `R5` high); programs set both halves, because
the hardware does not zero registers.

## Subset and limits

`MOV`, `IADD3`, `LOP3.LUT`, `FADD`, `FMUL`, `FFMA`, `ISETP`, `BRA`, `EXIT`, `LDG.E.32`,
`STG.E.32`. The CPU reference models round-to-nearest only and refuses NaN results and
uninitialized loads (UNKNOWN, see the research note §4).

Not yet in the corpus, and needed before T-023 can run programs on hardware: kernel
parameters through constant-bank operands, thread IDs (`S2R`), and a code-object container
the NVIDIA driver can load. Programs use fixed addresses, which T-023 must map.
