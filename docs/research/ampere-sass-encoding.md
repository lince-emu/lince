# Ampere (SM 8.x) SASS encoding — subset used by the T-014 corpus

- **Source:** Mesa, `https://gitlab.freedesktop.org/mesa/mesa`, branch `main`, commit
  `dd1b7c3f9520e175a2d517f307dd4ccdb7eb9433` (2026-11-13), file
  `src/nouveau/compiler/nak/sm70_encode.rs` (MIT, © Collabora). That file encodes Volta
  through Blackwell; only the paths taken for SM 8.x (`sm >= 80`, `sm < 100`) are recorded.
  Line numbers below refer to that commit.
- **Evidence level:** SRC. Mesa's authors test these encodings against NVIDIA's `nvdisasm`
  (Mesa's NVK hardware documentation); Lince does not, and may not (LEGAL-15, D-010). The use
  of this source was decided in [D-011](../decisions/D-011-sass-encoding-source.md).
- **Applies to Lince:** yes — the bit positions and opcode values for the T-014 assembler and,
  later, the T-018 decoder. No Mesa code is copied or translated (LEGAL-20/21/22); Lince's
  assembler is written from the facts below.

## 1. Instruction word

Each instruction is 128 bits, stored as four little-endian 32-bit words; bit *n* is bit
`n % 32` of word `n / 32` (`SM70Encoder::inst: [u32; 4]` L22–26, `encode_sm70_shader` L4815–4849: each
instruction appends four `u32`).

| Bits | Field | Source |
|------|-------|--------|
| 0–11 | opcode (ALU ops: 0–8 opcode, 9–11 operand form) | `set_opcode` L70; `encode_alu_base` L673–674 |
| 12–14 | guard predicate register (7 = `PT`) | `set_pred` L279–290 |
| 15 | guard predicate negated | `set_pred` L290 |
| 16–23 | destination GPR (255 = `RZ`) | `set_dst` L292–298; `zero_reg` L57–64 |
| 105–108 | stall cycles ("delay") | `set_instr_deps` L325 |
| 109 | yield | L330 (`sm < 120`) |
| 110–112 | write scoreboard set by this instruction (7 = none) | L326 |
| 113–115 | read scoreboard set by this instruction (7 = none) | L327 |
| 116–121 | scoreboards waited on (mask) | L328 |
| 122–125 | operand reuse mask | L331 |

`PT` is predicate 7 and `RZ` is GPR 255 (`true_reg` L66–68, `zero_reg`).

## 2. ALU operand forms

`encode_alu_base` (L613–675) places `src0` in a GPR field and chooses the form from `src1` and
`src2`:

| Form (bits 9–11) | `src1` | `src2` |
|------------------|--------|--------|
| 1 | GPR (or absent) | GPR (or absent) |
| 2 | GPR, in the `src2` field | 32-bit immediate, in the `src1` field |
| 4 | 32-bit immediate | GPR (or absent) |

| Operand field | Bits | Abs / neg bits | Source |
|---------------|------|----------------|--------|
| `src0` GPR | 24–31 | 73 / 72 | `encode_alu_src0` L543–555 |
| `src1` GPR | 32–39 | 62 / 63 | `encode_alu_reg` L579–581 |
| 32-bit immediate | 32–63 | — | `encode_alu_imm` L597–599 |
| `src2` GPR | 64–71 | 74 / 75 | `encode_alu_src2` L557–577 |

## 3. Instructions

| Mnemonic | Opcode | Operands (by §2) | Other fields | Source |
|----------|--------|------------------|--------------|--------|
| `FADD` | 0x021 | `a`, `b` as `src0`, `src1`; an immediate `b` goes to form 2 with `src1` = `RZ` | 77 saturate, 78–79 rounding, 80 FTZ | `OpFAdd` L849–888 |
| `FMUL` | 0x020 | `a`, `b` as `src0`, `src1`; `src2` = `RZ` | 76 DNZ, 77 sat, 78–79 rounding, 80 FTZ, 84–86 = 4 | `OpFMul` L963–996 |
| `FFMA` | 0x023 | `a`, `b`, `c` as `src0`–`src2` | as `FMUL` (no 84–86) | `OpFFma` L890–923 |
| `IADD3` | 0x010 | `a`, `b`, `c` | carry-in predicates 87–89/90 and 77–79/80 (`!PT` when unused); carry-out predicates 81–83, 84–86 (`PT` when unused) | `OpIAdd3` L1595–1649 |
| `LOP3.LUT` | 0x012 | `a`, `b`, `c` | 72–79 LUT, 80 = 0 (`.PAND`), 81–83 = 7, 87–89/90 = `!PT` | `OpLop3` L2067–2125 |
| `MOV` | 0x002 | value as `src1` (no `src0`, no `src2`) | 72–75 quad-lane mask (0xF = all) | `OpMov` L2420–2449 |
| `ISETP` | 0x00C | `a`, `b` as `src0`, `src1`; no destination GPR | 68–70/71 low-compare predicate (`PT`), 72 `.EX`, 73 signed, 74–75 combine op (0 `AND`), 76–78 compare, 81–83 destination predicate, 84–86 second destination (`PT`), 87–89/90 accumulator predicate (`PT`) | `OpISetP` L1869–1924 |
| `BRA` | 0x947 | — | 32 `.U` = 0, 34–81 relative offset, 87–89/90 condition predicate | `OpBra` L4194–4219 |
| `EXIT` | 0x94D | — | 84, 85 = 0, 87–89 = 7, 90 = 0 | `OpExit` L4221–4235 |
| `LDG` | 0x981 | — | 16–23 destination, 24–31 address GPR, 32–37 uniform address (`URZ` = 63), 40–63 signed offset, 64–66/67 predicate (stored as 7 − index), 72 uniform address is 64-bit, 73–75 size, 77–80 memory order, 81–83 = 7, 84–86 eviction priority, 90 address GPR is 64-bit, 91 = 1 | `OpLd` L3431–3506 |
| `STG` | 0x986 | — | 24–31 address GPR, 32–39 data GPR, 40–63 offset, 64–69 uniform address, 72, 73–75 size, 77–80 order, 84–86 eviction, 90, 91 as `LDG` | `OpSt` L3628–3702 |

Field values used by the corpus:

- **Compare (`ISETP` 76–78):** `LT` 1, `EQ` 2, `LE` 3, `GT` 4, `NE` 5, `GE` 6
  (`set_int_cmp_op` L1034–1049).
- **Rounding (78–79):** `RN` 0, `RM` 1, `RP` 2, `RZ` 3 (`set_rnd_mode` L765–776).
- **Memory size (73–75):** `B32` 4 (`set_mem_type` L3301–3315).
- **Memory order (77–80, SM 8.x):** weak 0 (`set_mem_order` L3273–3283).
- **Eviction priority (84–86):** normal 1 (`set_eviction_priority` L3287–3298).
- **Uniform address:** when unused it is `URZ` (63 before SM 10.0) with bit 72 set
  (`set_ureg_addr` L144–160, `ugpr_max` L49–55).
- **Branch offset:** `target − current − 4`, both counted in 32-bit words from the start of
  the program, i.e. relative to the next instruction (`get_rel_offset` L4089–4098; the
  instruction pointer advances by 4 per instruction, L4829).

## 4. Semantics used by the CPU reference (`poc/sass-corpus/src/exec.rs`)

| Behavior | Source | Evidence |
|----------|--------|----------|
| `LOP3` table: result bit = `lut[a·4 + b·2 + c]`, i.e. the function applied to `a = 0xF0`, `b = 0xCC`, `c = 0xAA` gives the table | NVIDIA, *PTX ISA*, `lop3` (docs.nvidia.com/cuda/parallel-thread-execution, PTX 9.5), immLut description and examples; that SASS bits 72–79 hold the same table follows from NAK storing the IR's table unchanged (`OpLop3` L2121) | DOC + SRC |
| `FADD`/`FMUL` round once, `FFMA` rounds once after the exact `a·b + c`; RN = round to nearest, ties to even; no flush-to-zero when bit 80 is clear | IEEE 754-2008 binary32; rounding/FTZ fields as in §3 | DOC |
| `IADD3`: 32-bit wrapping sum of three operands; `ISETP` `.U32`/`.I32` compares as unsigned/signed | Instruction definitions (NAK `OpIAdd3`, `OpISetP` field meanings) | SRC |
| `LDG`/`STG` address: 64-bit register pair (low in `Ra`, high in `Ra+1`) plus the signed 24-bit offset | NAK `set_reg_addr` bit 90 (64-bit address), `OpLd`/`OpSt` offset field | SRC |
| NaN result bit patterns, directed rounding results, uninitialized memory | No source the project may use | UNKNOWN — the CPU reference refuses them |

## 5. Not covered

Constant-bank operands, uniform registers as ALU sources, `S2R` (thread and block IDs),
shared and local memory, 64-bit loads, `MUFU`, textures and every other instruction. They are
added here, with sources, when the corpus needs them.
