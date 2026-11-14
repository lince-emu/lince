# LCIR v1 — Lince CPU IR

Status: Proposed (needs maintainer approval before the first JIT tier, T-038) · Related ADRs:
0004, 0005, 0019, 0030, 0031 · Research: `docs/research/cpu-jit-scheduling.md`

## 1. Purpose and scope

LCIR is the contract between the A64 front end and Lince's own backend (ADR-0005):

```text
A64 decoder → LCIR region → backend → code + relocations + fault metadata → Rust publishes
```

v1 is the minimum Tier B needs to lower the B1 and B2 instruction sets. It has no
optimization passes and no features that only Tier C would use. Tier A does not use LCIR: it
interprets decoded instructions directly and is the oracle LCIR is tested against.

Non-goals for v1: SIMD and floating point, PAC/MTE beyond what the decoder already reduces to
plain operations (ADR-0004), multi-block regions, loops inside a region, profiling counters.

## 2. Requirements

Each rule in this document traces to one of these.

| Id | Requirement | Source |
|----|-------------|--------|
| L1 | Plain data: one contiguous byte buffer per region, no pointers, crosses the C ABI in one call | ADR-0005, ARCH-50 |
| L2 | Deterministic: the same guest bytes and versions give the same bytes, bit for bit | ADR-0005 (cache), ADR-0004 (deterministic clock) |
| L3 | Hashable under a canonical encoding; versioned | doc 02 §keys, ADR-0005 |
| L4 | Bounded: fixed size limits; every pass over a region is linear | ADR-0005 (stutter) |
| L5 | Precise guest state at every exit and every fault | ADR-0005 (performance), ADR-0031 (fault lookup) |
| L6 | Memory operations carry size and ordering; exclusives and atomics are explicit | ADR-0004 (memory ordering) |
| L7 | Translation reads only the recorded snapshot of guest code, never live state | AGENTS.md invariant 6 |
| L8 | Guest registers and flags are values inside a region, so a backend may keep them in host registers | ADR-0005 |
| L9 | Semantics written from the Arm Architecture Reference Manual pseudocode, not from another emulator's IR | LEGAL-11, LEGAL-20/21 |

## 3. The region

A **region** is the translation unit: a run of consecutive guest instructions starting at one
guest address, ended by exactly one terminator (§7).

- **Single entry, single block.** Execution enters only at the first instruction. Straight-line
  code; the only way out before the terminator is a fault (§6.4). A guest loop therefore always
  passes through a region entry.
- **Where the front end stops.** After a branch, `SVC`, `BRK`, `ISB`, a hint that yields (`WFE`,
  `WFI`, `YIELD`), before an encoding Tier B does not lower (that instruction runs in Tier A), at
  the end of the code page, or at the limits below.
- **Limits (L4).** At most 64 guest instructions, 1024 operations and 256 pool constants per
  region. Operation indices then fit in 16 bits.
  `GUESS:` the values are starting points chosen to keep compile time small; they are tuned from
  measured compile times once a backend exists.
- **Dependency snapshot (L7).** The region header records the guest address and length of the
  bytes it was translated from. The front end reads those bytes from a snapshot taken when the
  request was queued, never from live guest memory. Invalidation of a code range finds every
  region whose recorded range overlaps it.

## 4. Types and values

Three types: `b1` (one bit), `i32`, `i64`. Narrow loads zero- or sign-extend straight into
`i32` or `i64` (§6.1), so no narrower integer type is needed.

Values are in SSA form. The value an operation defines is named by the operation's index in the
region, so there is no separate naming table (L2). An operand must name an earlier operation
that defines a value of the type the operation expects; the verifier (§9) checks both.

## 5. Guest state

Guest state is reached only through numbered **slots**:

| Slot | State | Type |
|------|-------|------|
| 0–30 | X0–X30 | `i64` |
| 31 | SP | `i64` |
| 32–35 | N, Z, C, V | `b1` |
| 36 | TPIDR_EL0 | `i64` |

- `get slot` → value; `set slot, value`.
- `XZR` is not a slot: the front end uses the constant 0 and drops writes to it. Writes to `Wn`
  are zero-extended to 64 bits by the front end (Arm ARM, `X[]` assignment).
- The flags are four separate `b1` slots. Instructions that set some flags and read others need
  no packing, and a backend can keep each flag wherever it is cheapest (L8).
- **The PC is not a slot.** Every terminator and every operation that may fault knows its guest
  PC from the region's entry address and its instruction ordinal (§8), so the PC is never
  computed at run time inside a region.
- Read-only system registers (`TPIDRRO_EL0`, `CNTVCT_EL0`, `CNTFRQ_EL0`) are read with `sysreg`
  (§6.3), not slots, because their values come from the runtime.

## 6. Operations

Every operation belongs to one guest instruction: it records that instruction's ordinal in
the region (`insn`). Operations of one instruction are contiguous and in instruction order.

### 6.1 Values and arithmetic

| Operation | Result | Semantics |
|-----------|--------|-----------|
| `const k` | `i32`/`i64`/`b1` | `k` from the constant pool |
| `add a, b` · `sub a, b` | `iN` | modulo 2^N |
| `adc a, b, c` | `iN` | `a + b + c` modulo 2^N, `c: b1` (Arm ARM `AddWithCarry` result) |
| `adc.c a, b, c` · `adc.v a, b, c` | `b1` | carry out and signed overflow of the same sum (`AddWithCarry` C and V) |
| `and` · `or` · `xor` | same type | bitwise; also on `b1` |
| `not a` | same type | bitwise complement |
| `lsl a, s` · `lsr a, s` · `asr a, s` · `ror a, s` | `iN` | shift amount `s` taken modulo N (the Arm ARM rule for register-specified shifts); immediate shifts pass a constant |
| `zext a` · `sext a` · `trunc a` | `i64`/`i64`/`i32` | between `i32` and `i64`; `zext` also takes `b1` |
| `eq a, b` · `neg a` · `zero a` | `b1` | equality, most significant bit, all bits zero |
| `select c, a, b` | type of `a` | `c ? a : b`, `c: b1` |
| `mul a, b` · `umulh a, b` · `smulh a, b` | `iN` / `i64` | low half; high half of the 128-bit product (B2) |
| `udiv a, b` · `sdiv a, b` | `iN` | quotient rounded toward zero; divisor 0 gives 0; `sdiv` of the minimum by −1 gives the minimum (Arm ARM `UDIV`/`SDIV`) (B2) |
| `clz a` · `rev a` · `rbit a` | `iN` | count leading zeros, byte reverse, bit reverse (B2) |

Subtraction that sets flags is lowered as the Arm ARM writes it:
`adc a, not b, 1`, with `adc.c` and `adc.v` for C and V, `neg` for N and `zero` for Z.
Condition codes (`B.cond`, `CSEL`) are lowered to `b1` operations on the flag slots, following
the Arm ARM `ConditionHolds` function. Bit-field and logical-immediate instructions are lowered
to shifts, masks and constants; the masks come from the decoder's `DecodeBitMasks`, the same
code Tier A uses.

### 6.2 Memory

| Operation | Result | Fields |
|-----------|--------|--------|
| `load addr` | `i32`/`i64` | size 1/2/4/8, extension (zero/sign), order |
| `store addr, value` | — | size 1/2/4/8, order |
| `probe addr` | — | byte count (≤ 16), access (read/write) |
| `load.x addr` | `i32`/`i64` | size, order: load exclusive, arms the monitor |
| `store.x addr, value` | `i32` | size, order: 0 on success, 1 on failure |
| `clrex` | — | disarms the monitor |
| `rmw addr, value` | `iN` (old value) | size, order, op (add, clr, eor, set, smax, smin, umax, umin, swp) (B2, LSE) |
| `cas addr, expected, new` | `iN` (old value) | size, order (B2, LSE) |
| `fence` | — | kind: loads, stores, full |

- **Order** is one of `plain`, `acquire`, `acquire.pc` (`LDAPR`), `release`. The backend must
  give at least the guest's ordering (ADR-0004) and must not add barriers to `plain` accesses.
  `DMB` maps to `fence` with the matching kind; `DSB` is a full `fence`.
- **Addresses** are `i64` guest virtual addresses. How they become host accesses (inline through
  the host base, fastmem, or the slow-path callback, ADR-0019/0031) is the backend's choice and
  is not visible in LCIR.
- **Pairs.** `LDP`/`STP` must not change anything when either half faults (the Tier A rule). The
  front end emits one `probe` of the whole pair first, then two accesses marked `nofault`. A
  `nofault` access is valid only when a `probe` of the same instruction covers it; the verifier
  checks that.
- **Exclusive monitor.** `store.x` succeeds only when the monitor of this core is armed for the
  same address and size and the memory still holds the value `load.x` read; it then disarms the
  monitor either way. This is ADR-0004's compare-and-swap emulation stated as a rule; its ABA
  limit is listed in ADR-0004's risks, not solved here.

### 6.3 System and time

| Operation | Result | Notes |
|-----------|--------|-------|
| `sysreg id` | `i64` | `TPIDRRO_EL0`, `CNTVCT_EL0`, `CNTFRQ_EL0` |

`CNTVCT_EL0` under the deterministic clock (ADR-0004) depends on the instructions executed so
far. The runtime computes it from the instruction count at region entry plus this operation's
`insn` ordinal, so a region gives the same counter value in Tier A and Tier B.

### 6.4 Faults

`load`, `store`, `probe`, the exclusives, `rmw` and `cas` may fault (data abort). A fault in
instruction `k` leaves the guest state exactly as after instruction `k − 1`: the PC is the
address of instruction `k`, every slot written by instructions before `k` holds its new value,
and nothing written by instruction `k` is visible (the Tier A rule, L5). The front end
therefore emits every `set` and every non-faulting store of an instruction after all of its
faulting operations; the verifier checks that order. How the backend makes earlier slot values
visible at a fault (storing them before the access, or recording where they live in the fault
metadata) is its choice.

## 7. Terminators

The last operation of a region is exactly one terminator. Each terminator records the guest
PC execution continues at and the number of guest instructions the region completed on that
path, which the deterministic clock and the frame ledger use.

| Terminator | Meaning |
|------------|---------|
| `jump target` | direct branch or fall-through; `target` is a pool constant |
| `branch c, taken, next` | two direct targets, `c: b1` |
| `jump.ind addr` | indirect branch (`BR`, `BLR`, `RET`); `ret` hint flag for `RET` |
| `svc imm` | returns to Rust for kernel dispatch; resumes at the next instruction |
| `trap kind` | `BRK`, or an instruction that must be raised as UNDEFINED |
| `yield kind` | `ISB`, `WFE`, `WFI`, `YIELD`: returns to the core's run loop |

- **Direct targets are patch sites.** The backend reports one patch site per direct target, and
  Rust links or unlinks it (ADR-0031). The front end never assumes the target is translated.
- **Polling.** A region checks its core's halt flag at entry, before its first instruction.
  Regions have no internal loops and at most 64 instructions, so the time between checks is
  bounded by one region. Halt, interrupt delivery and cooperative invalidation (research note
  F5) use that check.
- `yield` after `ISB` is how a guest's own code modification (the guest writes code, then runs
  `ISB`) reaches the dispatcher, where pending invalidations are applied.

## 8. Serialized form

One little-endian buffer (L1). The canonical form is the only valid form (L2, L3).

```text
header    32 bytes
  magic        u32  "LCIR"
  version      u16  format version, bumped on any change to this document
  flags        u16  0 in v1
  guest_va     u64  address of the first instruction
  guest_len    u32  bytes translated (4 × instructions)
  insn_count   u16
  op_count     u16
  pool_count   u16
  reserved     u16 × 3, zero
pool      pool_count × u64     constants, in first-use order, no duplicates
ops       op_count × 16 bytes
  opcode       u8
  type         u8   b1/i32/i64, or 0 when no value is defined
  aux          u16  size, extension, order, kind, slot or flags, by opcode
  insn         u16  guest instruction ordinal in the region
  a, b, c      u16  operand op indices or pool indices, by opcode; unused = 0
  reserved     u32  zero
```

- Fixed-size records keep every pass a linear scan and the offset of op `i` at
  `32 + 8 × pool_count + 16 × i`.
- Text form for tests and diagnostics: `%5 = adc.i64 %3, %4, %2 @1`, where `@1` is the
  instruction ordinal. It is a printing of the bytes, not a second format.
- **Cache key** (ADR-0005, doc 02): XXH3-128 over the guest bytes of the region, the guest
  address, the LCIR format version, the front-end version and the backend version. The XXH3 hash
  of the LCIR buffer is stored with the entry. CI retranslates and compares it, which checks L2.

## 9. Verifier

A single linear pass, run on every region in debug builds, tests and fuzzing:

- limits of §3; header fields consistent with the buffer length;
- every operand names an earlier op of the right type, or a pool entry in range;
- `insn` ordinals do not decrease; within one instruction, no `set` or non-`nofault` store
  precedes a faulting operation (§6.4); every `nofault` access lies inside a `probe` of the
  same instruction;
- exactly one terminator, last;
- reserved fields zero, pool deduplicated and in first-use order (canonical form).

## 10. Example

`ADDS X0, X1, #1` then `CBZ X0, 0x8000040`, region at `0x8000000`:

```text
pool: [0] 1  [1] 0  [2] 0x8000040  [3] 0x8000008
%0 = get.i64 x1 @0
%1 = const.i64 [0] @0
%2 = const.b1 [1] @0
%3 = adc.i64 %0, %1, %2 @0
%4 = adc.c %0, %1, %2 @0
%5 = adc.v %0, %1, %2 @0
%6 = neg %3 @0
%7 = zero %3 @0
     set x0, %3 @0
     set n, %6 @0
     set z, %7 @0
     set c, %4 @0
     set v, %5 @0
%13 = zero %3 @1
      branch %13, [2], [3] @1
```

## 11. Hazards carried from the research note

From `docs/research/cpu-jit-scheduling.md`, listed as hazards, not as designs to follow:

- **Patching (F4).** Linking writes into published code while other cores may run it. LCIR only
  names the patch sites; Rust patches them (ADR-0031) with an atomic write of an aligned branch.
- **Invalidation (F5).** Invalidation is requested from any thread and applied when the core
  next passes a poll (§7). The region's recorded guest range (§3) is what invalidation matches.
- **Whole-cache flush (F2).** Never on a guest core. Eviction is the code cache's concern, done
  on workers; a region whose code is evicted falls back to Tier A until retranslated.

## 12. Validation plan

1. A reference evaluator of LCIR (test-only, C++) runs regions against the same state and memory
   as Tier A; differential tests compare every lowered instruction with Tier A.
2. Fuzzing: random instruction sequences from the decoder table are translated, verified and
   evaluated against Tier A.
3. When the backend exists, its code is compared with the evaluator on the same regions.
4. Determinism: translating the same guest bytes twice gives identical buffers (L2).

## 13. Open questions for the maintainer

1. Region limits (64 instructions, 1024 ops): accept as starting values to tune by measurement?
2. LSE atomics (`rmw`, `cas`) in v1, or only when a B2 program uses them?
3. Flags as four `b1` slots (chosen here) or one packed NZCV slot?
4. Pairs as `probe` + two `nofault` accesses (chosen here) or a dedicated pair operation?
