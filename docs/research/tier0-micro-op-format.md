# Tier-0 micro-op format (proposal)

- **Status:** Proposal — input for ADR-0010 and PoC-2 (T-017). Not binding until ADR-0010 is
  Accepted; PoC-2 may change any field.
- **Format version:** `interp_format = 1` (draft). Part of the cache L2 key (doc 02 §2).
- **Source:** Lince design documents (doc 02 §2, doc 03 §4, ADR-0009, ADR-0010); the guest-ISA
  facts in §11, each with its own source.
- **Evidence level:** RECALL for F1; UNKNOWN (working assumption, no evidence yet) for F2–F10.
  Everything else is Lince design, not a research claim.
- **Applies to Lince:** yes — defines the data the Tier-0 GPU interpreter consumes and the
  output of the micro-op pre-decoder.

This note specifies **Lince's own** data format. It contains no SASS bit encodings; those
belong in a future `ampere-sass-encoding.md` with per-field evidence (PAT-10). Nothing here is
derived from another emulator's code or from leaked material (LEGAL-10, LEGAL-20).

## 1. Goals and non-goals

Goals (doc 03 §4.1):

1. **No per-program host compile.** The micro-op stream is plain data in a storage buffer,
   consumed by global interpreter pipelines (doc 03 §4.6).
2. **Linear, allocation-free pre-decode.** One pass over the guest program, output size known
   before decoding starts, so it can run at discovery time or, bounded, on the render path
   (§10).
3. **Uniform dispatch on the GPU.** Every lane of an emulated warp that executes in an
   iteration reads the *same* micro-op, so the opcode `switch` is in uniform control flow.
4. **State independence.** A stream is a pure function of (guest program image,
   `interp_format`). Anything that depends on guest state — texture types and formats,
   constant-buffer values, viewport mode, compute launch parameters — is read at run time from
   per-draw or per-dispatch parameter blocks (doc 03 §4.2). There is one stream per program,
   never one per variant, and pre-decode never reads live guest state (ARCH invariant 6).
5. **Faithful semantics.** Micro-ops keep the guest's operation, operands and modifiers; no
   arithmetic is reassociated or contracted (ARCH-14).

Non-goals: being a compiler IR (that is Lince IR, ADR-0009); optimizing the guest program;
executing no-fallback classes (doc 03 §6) — they are flagged, not executed.

## 2. Program container

A stream is one little-endian byte buffer, uploaded once per program and addressed by the
interpreter through a device address or a descriptor-heap index.

| Offset | Size | Region |
|--------|------|--------|
| 0 | 64 B | Header (§2.1) |
| 64 | 16 B × `uop_count` | Micro-op array (§3), 16-byte aligned |
| 64 + 16·`uop_count` | 4 B × `pool_words` | Auxiliary pool (§5) |

A **source map** (micro-op index → guest byte offset and raw instruction bits) is produced
alongside the stream but kept on the CPU; it is used for diagnostics only and never uploaded.

### 2.1 Header

| Field | Type | Meaning |
|-------|------|---------|
| `magic` | u32 | Fixed tag identifying a Lince micro-op stream |
| `format_version` | u16 | `interp_format`; a reader treats any other value as a miss (§9) |
| `stage` | u8 | Vertex, tessellation control, tessellation evaluation, geometry, fragment, compute, task, mesh |
| `gpr_bucket` | u8 | Register-file bucket (§4.1) |
| `uop_count` | u32 | Number of micro-ops = number of guest instruction slots (§6) |
| `pool_words` | u32 | Size of the auxiliary pool in 32-bit words |
| `flags` | u32 | Program properties (below) |
| `local_mem_bytes` | u32 | Graphics stages: per-lane local memory from the program header (F6). Compute: always 0; the size comes from the per-dispatch parameter block (F7) |
| `max_gpr` | u8 | Highest GPR index read or written, including pair/quad widths; the zero register is excluded; 0 when no GPR is used |
| `max_ugpr` | u8 | Highest uniform register index used, zero register excluded; 0 when none |
| `reserved0` | u16 | Zero |
| `entry` | u32 | Micro-op index of the entry point (normally 0) |
| `interface` | u32 | Pool word offset of the stage-interface record (§5.2); `0xFFFF_FFFF` for compute |
| `program_id` | 16 B | `GuestProgramId` of the source image, for validation |
| `reserved1` | 12 B | Zero |

`flags` bits:

- `NO_FALLBACK_CLASS` — the program belongs to a class on the no-fallback list (doc 03 §6), or
  uses an instruction family classified as no-fallback under PAT-10 step 3. This is the **only**
  flag the tier selector uses to keep a program off Tier 0.
- `HAS_TRAP` — informational: the stream contains at least one `TRAP` (§7). The program still
  runs on Tier 0; ADR-0009 keeps programs with `Unimplemented` ops on Tier 0, so Tier 0 must
  not reject them.
- `USES_CTA_BARRIER`, `USES_WARP_OPS`, `USES_DERIVATIVES`, `USES_LOCAL_MEMORY`,
  `USES_SHARED_MEMORY`, `USES_UNIFORM_DATAPATH`, `USES_INDIRECT_BRANCH`, `USES_FP64`,
  `MAY_KILL` — let the interpreter skip setup it does not need and let PoC-2 attribute cost
  per class.

## 3. Micro-op encoding

Every micro-op is **128 bits = four u32 words** (`w0`–`w3`), read with one 16-byte load. No
field crosses a word boundary, so the interpreter extracts each field with a single shift and
mask.

| Word | Bits | Field | Meaning |
|------|------|-------|---------|
| `w0` | 0–9 | `op` | Micro-op opcode (§3.4), up to 1024 |
| `w0` | 10–14 | `guard` | Predicate operand (§3.2) that guards execution; `PT` = always |
| `w0` | 15–24 | `dst` | Register operand (§3.1) written by the operation |
| `w0` | 25–30 | `src_mods` | Two bits per source A, B, C; meaning set by the opcode family (float: negate, absolute; integer: negate, bitwise not) |
| `w0` | 31 | — | Reserved, zero |
| `w1` | 0–9 | `src_a` | Register operand |
| `w1` | 10–19 | `src_b` | Register operand |
| `w1` | 20–29 | `src_c` | Register operand |
| `w1` | 30–31 | — | Reserved, zero |
| `w2` | 0–4 | `pdst0` | Predicate operand written (comparison result, carry out); `PT` = none |
| `w2` | 5–9 | `pdst1` | Second predicate written; `PT` = none |
| `w2` | 10–14 | `psrc0` | Predicate operand read (combine input, carry in, selector); `PT` = true |
| `w2` | 15–19 | `psrc1` | Second predicate read; `PT` = true |
| `w2` | 20–31 | `mods` | 12 opcode-specific modifier bits (comparison, rounding, type, logic table, memory size, …) |
| `w3` | 0–31 | `wide` | Immediate, constant-buffer reference, branch target or pool offset (§3.3) |

The layout covers the operand shape assumed for common ALU, comparison and carry
instructions (F8): one register destination, three register sources, two predicate outputs,
two predicate inputs and one 32-bit wide value. Anything beyond that shape has a defined
overflow:

- **More predicate inputs** (e.g. three-input predicate logic): opcodes that take only
  predicates carry them in the register slots with bank AUX (§3.1), `index` holding a 5-bit
  predicate operand (§3.2).
- **More than 12 modifier bits, 64-bit immediates, texture operations, constant-buffer
  references that do not fit §3.3:** a pool record (§5) referenced by `wide`.

### 3.1 Register operand (10 bits)

| Bits | Field | Values |
|------|-------|--------|
| 0–7 | `index` | Register number in the selected bank |
| 8–9 | `bank` | `0` GPR · `1` UGPR (uniform register) · `2` WIDE · `3` AUX |

- **GPR:** `index` 0–254 selects a general register; **255 is the zero register** (reads 0,
  writes discarded).
- **UGPR:** `index` selects a uniform register; **255 is the uniform zero register**, whatever
  number the guest uses for it (F3). The encoding therefore does not depend on the guest's
  uniform file size.
- **WIDE:** the value is `w3`, interpreted by the opcode as an immediate or a constant-buffer
  reference (§3.3); `index` must be 0.
- **AUX:** `index` is a small opcode-defined value: a system-value id for special-register
  reads, or a predicate operand for predicate-only opcodes. `index = 255` means the slot is
  absent.

Multi-register values (64-bit operands, vector loads and stores) name the **first** register;
the width comes from the opcode and `mods`. The pre-decoder checks the alignment the guest ISA
requires; a violation becomes `TRAP` (§7) rather than a guess.

### 3.2 Predicate operand (5 bits)

| Bits | Field | Values |
|------|-------|--------|
| 0–2 | `index` | 0–6 select a predicate; 7 is the constant-true predicate |
| 3 | `bank` | `0` per-lane predicates · `1` uniform predicates |
| 4 | `negate` | Invert the value read |

Encoding `0b00111` is "always true" (unguarded). Writing to index 7 discards the result.
Negated index 7 reads as false, which makes a micro-op a no-op without changing its slot.

### 3.3 The wide field

`w3` has exactly one meaning per opcode, fixed in the opcode table:

| Kind | Content |
|------|---------|
| Immediate | 32-bit value, used as-is (float bits or integer) |
| Constant-buffer reference | bits 0–15: byte offset · bits 16–20: buffer slot · bits 21–31: zero. A reference outside these ranges (F9) uses a pool record instead |
| Branch target | Absolute micro-op index (§6) |
| Pool offset | Word offset of an auxiliary record (§5) |
| Memory offset | Signed immediate added to a register address |

Constant-buffer *contents* are never copied into the stream: the interpreter reads them at
run time through the same address mechanism as Tier 1/2 (doc 03 §4.3). That keeps the stream
state-independent (goal 4).

### 3.4 Opcode space

Opcodes are numbered by Lince, not taken from guest encodings, and grouped by family in blocks
of 64 so the interpreter can branch to a family handler first:

| Range | Family |
|-------|--------|
| 0 | `TRAP` (unsupported or invalid; §7) — zero-filled memory therefore traps |
| 1 | `NOP` |
| 2–63 | Control flow, convergence and warp synchronization |
| 64–127 | Integer ALU and logic |
| 128–191 | 32-bit float ALU, special functions |
| 192–255 | 16- and 64-bit float, conversions |
| 256–319 | Predicate logic, moves, selects, system values |
| 320–383 | Memory: global, local, shared, constant, atomics |
| 384–447 | Texture and surface |
| 448–511 | Warp operations (shuffle, vote, match, reduce) and CTA barriers |
| 512–1023 | Reserved |

Rules: one guest instruction maps to one opcode; a guest instruction whose variants have very
different semantics (e.g. a wide multiply-add versus a 32-bit one) gets one opcode per variant
so the interpreter never decodes those variants in its hot loop. Mnemonics follow the
instruction-set reference in NVIDIA's public *CUDA Binary Utilities* documentation (LEGAL-12);
the documentation version consulted is pinned in `ampere-sass-encoding.md` together with the
CUDA toolkit used for T-014. The numbering is part of `interp_format`; any renumbering bumps it.

### 3.5 Illustrative example

A guarded fused multiply-add of the form "if not P1: R2 = R0 × cbuf[0][0x160] + R3" becomes:

| Field | Value |
|-------|-------|
| `op` | the 32-bit float FMA opcode |
| `guard` | index 1, bank 0, negate 1 |
| `dst` | GPR 2 |
| `src_a` | GPR 0 |
| `src_b` | WIDE |
| `src_c` | GPR 3 |
| `pdst0`, `pdst1` | constant-true (none) |
| `mods` | rounding mode, flush-to-zero, saturate as the guest instruction specifies |
| `wide` | constant-buffer reference: slot 0, offset `0x160` |

## 4. Register files and buckets

### 4.1 GPR buckets

Per-lane GPRs live in a local array in the interpreter. Allocating 256 registers for every
program wastes scratch memory and occupancy, so interpreter pipelines exist in a few
register-file sizes (doc 03 §4.3). The pre-decoder writes the smallest bucket that holds
`max_gpr + 1` registers (bucket 0 when no GPR is used):

| `gpr_bucket` | Registers |
|--------------|-----------|
| 0 | 32 |
| 1 | 64 |
| 2 | 128 |
| 3 | 256 |

The zero register is never stored; reads of index 255 return 0 in every bucket. The bucket
multiplies the number of global interpreter pipelines (stages × buckets × fragment numeric
classes, doc 03 §4.5). PoC-2 measures whether four buckets are worth it.

### 4.2 Predicates

The per-lane predicates and the uniform predicates (F2) are packed into one u32 per lane:
bits 0–6 per-lane, bits 8–14 uniform. Reads and writes are single bit operations.

### 4.3 Uniform registers and the uniform datapath

Uniform registers hold warp-uniform values. Proposed storage: **a per-lane replica** of the
uniform file (a second local array), written by every lane that executes a uniform-datapath
micro-op. This is correct only if uniform-datapath instructions execute with the warp
converged (F5). Alternatives for PoC-2 to compare:

| Option | Trade-off |
|--------|-----------|
| A. Per-lane replica (proposed) | Simple; costs registers per lane; wrong if a uniform op runs while lanes are diverged |
| B. Per-warp copy in workgroup shared memory | Exact under divergence; compute, task and mesh stages only |
| C. Per-lane replica, written with a subgroup broadcast from the executing lanes | Exact for diverged lanes in all stages; extra subgroup operation per uniform write |

A debug build of the interpreter checks uniform values with a subgroup all-equal test and
counts violations, which answers F5 on real programs.

## 5. Auxiliary pool

The pool is an array of u32 words after the micro-ops. Records are word-aligned; their layout
is defined per opcode family. Records are written in micro-op order, so the pool is produced
in the same single pass.

### 5.1 Record kinds

| Record | Used by | Content |
|--------|---------|---------|
| Wide immediate | 64-bit immediates | Two words |
| Extended modifiers | Instructions with more than 12 modifier bits | One or more words |
| Constant-buffer reference | References outside the §3.3 ranges | Slot and byte offset, one word each |
| Texture descriptor | Texture and surface operations | Dimensionality, array/shadow/offset flags, LOD mode, component mask, handle source (bound slot or bindless register), immediate offsets |
| Raw guest bits | `TRAP` | The 128 raw bits of the unsupported instruction (diagnostics, ADR-0009) |

### 5.2 Stage-interface record

For graphics stages: input attribute mask, output attribute mask, per-output interpolation
requirements (fragment), fragment output component masks and the expected numeric class of
each output. The values come from the guest program header (F6). The tier selector uses the
numeric classes to pick the interpreter variant (doc 03 §4.5).

## 6. Code range and branch targets

**Code range.** The program start is the first instruction after the graphics program header
(or the code base for compute). The L0 image's code is one contiguous range
`[start, start + 16·N)`, and `uop_count = N`. A program whose reachable code is not contiguous
is stored as the smallest contiguous range that covers it; unreachable slots inside it
pre-decode normally (or to `TRAP`) and are never executed.

**Requirement on the L0 extent scan (doc 02 §3).** A control-flow scan cannot follow indirect
branches, so a program that contains any indirect branch must not be cut at the last
statically reached instruction: its extent runs to the end of the linear scan of the code
region (or to the next program header). Otherwise valid switch-table targets would fall
outside the range and trap on every run. Only targets beyond that extent take the trap path.

**Mapping rule: micro-op index = (guest byte offset − start) / 16.** Every guest instruction
slot, including padding and no-ops, maps to exactly one micro-op at the same index. Volta+
instructions are a fixed 16 bytes with scheduling control embedded (F1), so the mapping is pure
arithmetic.

Consequences:

- **Direct branches** (relative or absolute): the pre-decoder writes the absolute target index
  into `wide`. Forward targets need no second pass, which keeps pre-decode single-pass.
- **Out-of-range or misaligned direct targets** make *that* micro-op a `TRAP` (§7). The rest of
  the program is unaffected; the branch may sit in dead code.
- **Indirect branches** (target in a register): the interpreter applies the same arithmetic at
  run time; a target outside `[start, start + 16·N)` or not 16-byte aligned takes the run-time
  trap path (§7). What base the guest's target register is relative to is unknown (F4).
- **Calls and returns:** modelled as a direct branch that writes a return index and an indirect
  branch that reads it. Where the guest keeps the return address is unknown (F4); the format
  does not need a call stack either way.
- **Convergence barrier setup** stores its reconvergence target in `wide` like a branch.
- Debugging and the source map are trivial: guest offset = start + index × 16.

## 7. Unsupported instructions and the trap path

- **Pre-decode time:** a guest instruction the format cannot express, or a direct branch with
  an invalid target, becomes `TRAP` with `wide` pointing to a raw-bits record, and the header
  gets `HAS_TRAP`. The program still runs on Tier 0 (ADR-0009); only `NO_FALLBACK_CLASS` keeps
  it off Tier 0 (§2.1). A `TRAP` in dead code costs nothing.
- **Executed `TRAP` and run-time failures** (indirect branch out of range, register index past
  the bucket, unreleasable barrier, helper watchdog, §8): the lane appends a record
  (program id, micro-op index, failure kind) to a **fixed-size diagnostics ring** shared by all
  Tier-0 work, whose atomic counter also counts records dropped on overflow, and the lane ends.
  No buffer is allocated per draw. The ring is read only after the timeline value of the
  submission that wrote it has completed, by a non-blocking poll on a thread that is not
  render-critical — never by waiting on the GPU (ARCH-05). That thread turns each record into a
  frame-ledger event (DEV-21).
- Ending the lane is a **defect path**, not an approximation mode (ARCH-02): it is never
  enabled by a setting, every occurrence is logged and counted, and the draw itself is never
  skipped (ARCH invariant 1). What a trapped lane would have written is lost; PoC-2 tracks how
  often this happens on the corpus (§12).

## 8. How the interpreter consumes the stream

Per emulated warp of 32 lanes (a host subgroup of 32, or emulated as in doc 03 §4.4):

```text
state per lane: pc, flags {exited, waiting(barrier id), at_cta_barrier, killed}, GPRs,
                predicates, uniform replica
state per warp: convergence-barrier masks (one 32-bit mask per barrier register)

loop:
    release every convergence barrier whose mask lanes are all waiting on it, exited or killed
    runnable = not exited and not waiting and not at_cta_barrier
    pc_min   = subgroup_min(runnable ? pc : NONE)              # uniform value
    if pc_min == NONE:
        if every lane is exited or at_cta_barrier: leave loop  # done, or CTA barrier phase
        else: trap path (§7) for the waiting lanes; end them; continue  # unreleasable barrier
    if warp has a killed lane and iterations since the first kill > watchdog:
        trap path (§7) for all live lanes; leave loop
    uop    = stream[pc_min]                                    # uniform load
    run    = runnable and pc == pc_min and guard(uop) holds
    switch family(uop.op), then uop.op:                        # uniform control flow
        execute with `run` as the lane mask
    pc = next pc for lanes with pc == pc_min (pc + 1, or the branch target where taken)
```

- **Uniform dispatch.** Because `pc_min` is warp-uniform, the opcode switch never diverges;
  divergence appears only as the `run` mask. This is what lets subgroup and quad operations
  run in uniform host control flow.
- **Reconvergence.** Executing the minimum PC keeps lanes together after forward branches and
  holds quads together for derivatives (doc 03 §4.4).
- **Convergence barriers.** Setup records the mask of runnable lanes in the barrier register;
  a lane reaching the matching synchronization becomes `waiting` until the release step finds
  every lane in the mask waiting there or exited. Exited lanes are removed from all masks.
  Warp-synchronization instructions wait the same way on their explicit mask.
- **Warp operations.** Every lane calls the host subgroup operation (uniform control flow), but
  only lanes in the guest's active set — `run`, or the instruction's explicit mask operand —
  contribute inputs; other lanes contribute the operation's neutral value (0 for ballots and
  sums, true for "all" votes, and so on) and discard the result. Exited, waiting and helper
  lanes are therefore invisible to guest-visible warp results.
- **CTA barriers (compute).** A lane reaching the barrier sets `at_cta_barrier`; when no lane
  is runnable and all live lanes are at the CTA barrier, the loop leaves, the shader executes
  the host barrier in uniform control flow, clears the flag and re-enters (doc 03 §4.4 phased
  loop).
- **Derivatives and helpers (fragment).** A lane killed by the guest stops contributing to
  guest-visible results and stops writing memory and outputs, but stays alive as a helper so
  the internal quad operations used for derivatives always see four lanes; implicit-LOD texture
  operations compute derivatives with quad swaps and use explicit gradients (doc 03 §4.5).
  Whether the guest's kill instruction terminates the lane or demotes it to a helper in the
  guest model is unknown (F10); this design matches the demote behavior and PoC-2 checks it.
  For synchronization a killed lane counts as **exited**: it leaves every convergence-barrier
  and warp-synchronization mask, so no barrier waits on it. Atomics on a killed lane are not
  performed and return 0; its stores and outputs are dropped, which is the guest-visible
  meaning of the kill.
- **Helper watchdog.** A helper whose loop condition depends on a value it no longer writes
  could keep `pc_min` alive forever and lose the device. So, only while the warp contains a
  killed lane, a per-warp counter of loop iterations (one iteration = one executed micro-op)
  runs; past a watchdog limit sized below the host GPU timeout (PoC-2), the warp takes the §7
  trap path. Warps without killed lanes are never cut short, so long but correct programs are
  unaffected. Hitting the watchdog is a defect path, not an approximation.
- **Operand fetch.** A register operand reads the GPR array, the uniform replica, `wide`, or a
  constant buffer through its slot address; source modifiers apply after the read.

## 9. Validation on read

A persisted L2 stream (doc 02 §2) is checked before it is uploaded. Any failure is a cache miss:
the stream is regenerated from L0 and the event is logged (doc 02 §9).

Header checks: `magic`; `format_version == interp_format`; `program_id` equals the lookup key;
record length `== 64 + 16·uop_count + 4·pool_words`; `gpr_bucket ≤ 3`;
`max_gpr < bucket size`; `entry < uop_count`; `interface` inside the pool or `0xFFFF_FFFF`
(and `0xFFFF_FFFF` exactly when `stage` is compute); reserved fields zero.

Body check: one linear pass over the micro-ops verifying that

- every branch-target `wide` is `< uop_count`;
- every pool-offset `wide` plus its record length is inside the pool;
- every GPR-bank operand is 255, or its first index plus the width the opcode implies fits the
  bucket;
- every UGPR-bank operand is 255 or `≤ max_ugpr`;
- every WIDE-bank operand has `index == 0`;
- every AUX-bank operand is 255 or within the opcode's declared range.

The pass costs O(N), runs on the same worker thread as the cache read, and never at startup
(ARCH-16). The interpreter additionally
bounds-checks `pc` before each fetch, so a stream that slipped through cannot read outside its
buffer.

## 10. Pre-decoder contract

- Input: the L0 program image (doc 02 §2). Output: the stream of §2 and the CPU-side source map.
- Output size is bounded before decoding: `64 + 16·N + 4·pool_bound(N)` bytes, where `N` is
  the number of instruction slots and `pool_bound(N)` is `N` times the largest per-instruction
  pool record in the opcode table.
- The output buffer and the GPU upload staging come from pre-sized per-frame arenas or rings;
  no allocation in steady state.
- Single forward pass; no instruction is read twice.
- Deterministic: identical input and `interp_format` give identical bytes, so streams can be
  hashed, cached as L2 and compared in tests.
- Never reads guest state other than the program image (ARCH invariant 6).
- Normally runs at discovery time on a worker (doc 03 §5). When it must run on the render path,
  it emits a frame-ledger event with the instruction count and elapsed time; its worst case is
  linear in `N` (bounded by the program size limit measured in PoC-2).
- Placement (doc 04): format types and the opcode table in `lince-shader-interp`; the
  pre-decoder in `lince-shader-sass-ampere`. Raw guest fields are decoded once at that boundary
  and unknown encodings become `TRAP`, never a default (PAT-03).

## 11. Guest-ISA facts this format relies on

| # | Fact | Evidence | How to confirm |
|---|------|----------|----------------|
| F1 | Volta-and-later instructions are 128 bits with scheduling control embedded in each instruction (no separate control words) | RECALL — Jia, Maggioni, Staiger, Scarpazza, *Dissecting the NVIDIA Volta GPU Architecture via Microbenchmarking*, arXiv:1804.06826 (2018) | Disassemble team-written sm_86 kernels (T-014, LEGAL-14). This confirms the Ampere generation, not the exact SM variant of the Switch 2 GPU (doc 08 §2) |
| F2 | 255 general registers plus a zero register; seven predicates plus a constant-true predicate; the same shape for uniform predicates | UNKNOWN — working assumption | T-014 corpus |
| F3 | Size of the uniform register file and numbering of its zero register | UNKNOWN — working assumption | T-014 corpus |
| F4 | Base of indirect-branch targets; where call return addresses live | UNKNOWN | T-014 corpus with indirect calls and switch tables |
| F5 | Uniform-datapath instructions execute only while the warp is converged | UNKNOWN | Debug interpreter check (§4.3) and T-014 corpus |
| F6 | Layout of the graphics program header (local memory size, stage I/O) on the Switch 2 GPU | UNKNOWN | Separate research note |
| F7 | Compute local and shared memory sizes come from launch state, not from the program | UNKNOWN — doc 08 §2 lists QMD 3.0 launches [EMU] but not their fields | `qmd3.md` research note |
| F8 | Maximum operands per instruction: one register destination, three register sources, one immediate or constant-buffer operand, two predicate outputs, two predicate inputs besides the guard (predicate-only logic excepted, §3) | UNKNOWN — working assumption | T-014 corpus: count operand shapes per opcode |
| F9 | Constant-buffer slots ≤ 32 and offsets < 64 KiB in direct constant-buffer operands | UNKNOWN — working assumption | T-014 corpus |
| F10 | Whether the guest's fragment kill instruction ends the lane or demotes it to a helper | UNKNOWN | T-014 corpus and PoC-2 derivative tests |

If F2, F3, F8 or F9 turn out wrong, the encoding still has a defined place for the data:
register indices are 8 bits with fixed zero-register codes, extra operands and wide
constant-buffer references go to the pool (§3, §5), and only the bucket table and the predicate
packing would change.

## 12. Open questions for PoC-2

1. 128-bit micro-ops versus a split "hot 64-bit + cold 64-bit" layout: interpreter decode cost
   and cache behavior.
2. Uniform register storage: options A–C in §4.3.
3. Bucket set: four buckets versus two (64/256) versus one, against pipeline count.
4. Persist L2 streams or regenerate them from L0 (doc 02 §2): pre-decode and validation cost
   per KiB.
5. Whether 12 inline modifier bits keep the pool out of the common ALU path (count pool
   lookups per executed micro-op on the corpus).
6. Interpreter behavior on hosts that cannot force subgroup size 32 (ADR-0010 risk 2).
7. How often the trap path (§7) ends lanes on the corpus, per failure kind, and the size of
   the helper watchdog limit (§8).
8. **Maintainer decision (doc 02 §3):** the §6 extent rule for programs with indirect branches
   changes which code bytes are hashed into `GuestProgramId`, a cache key input. Doc 02 §3 is
   not edited by this note.
