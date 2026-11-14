// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The C header of lince-cpu-a64: the whole Rust/C++ contract of the crate (ADR-0030, ARCH-50).
//
// Only extern "C" functions, plain structs of fixed-size fields and opaque pointers cross the
// boundary. Every function is noexcept on the C++ side. Ownership (ARCH-53): every pointer
// argument is owned by the caller and only borrowed for the duration of the call.

#ifndef LINCE_CPU_A64_H
#define LINCE_CPU_A64_H

#include <stdint.h>

#ifdef __cplusplus
#define LCA_NOEXCEPT noexcept
extern "C" {
#else
#define LCA_NOEXCEPT
#endif

// An instruction id from the description table (cpp/src/insns.inc). 0 means the word matches
// no entry: an unknown encoding is reported, never guessed (ADR-0004). Ids are not stable
// across versions of the table; persist names, not ids.
typedef uint16_t lca_insn;
static const lca_insn LCA_INSN_UNKNOWN = 0;

// Longest instruction name, in bytes, without a terminator.
#define LCA_INSN_NAME_MAX 32

// The instruction id of the A64 word `word`, or LCA_INSN_UNKNOWN.
lca_insn lca_decode(uint32_t word) LCA_NOEXCEPT;

// Copies the ASCII name of `insn` into `out`, which holds LCA_INSN_NAME_MAX bytes (not
// terminated), and returns its length; returns 0 and writes nothing when `insn` is
// LCA_INSN_UNKNOWN, out of range, or `out` is null.
uint32_t lca_insn_name(lca_insn insn, char *out) LCA_NOEXCEPT;

// ---- Tier A interpreter (ADR-0005) ----

// Status codes. A fixed-width integer, not an enum, so its size is the same in C, C++ and Rust.
typedef int32_t lca_status;
static const lca_status LCA_OK = 0;
static const lca_status LCA_NULL_ARGUMENT = 1;

// Guest permission bits, the same values as lince-mem's Permission.
static const uint32_t LCA_PERM_READ = 1;
static const uint32_t LCA_PERM_WRITE = 2;
static const uint32_t LCA_PERM_EXECUTE = 4;

// One mapped guest region: [base, base + size) with `perm`.
typedef struct lca_region {
  uint64_t base;
  uint64_t size;
  uint32_t perm;
  uint32_t reserved;
} lca_region;

// The slow path for data accesses (ADR-0031): called for an access that no single listed
// region holds with the needed permission. Each function copies all `size` bytes and returns
// 0, or returns nonzero and copies nothing when any byte cannot be accessed (a data abort).
// A null function pointer makes every such access a data abort. `context` is passed back
// unchanged; the callbacks may not change the mapped regions.
typedef struct lca_callbacks {
  void *context;
  int32_t (*read)(void *context, uint64_t va, uint8_t *out, uint64_t size);
  int32_t (*write)(void *context, uint64_t va, const uint8_t *data, uint64_t size);
} lca_callbacks;

// Guest memory for one lca_run call (ADR-0031: accessed inline through the host base). Guest
// byte `va` of a listed region is at host_base + va; host_size bounds every such address.
// Addresses outside the regions, or without the needed permission, are never touched inline;
// data accesses to them go through `callbacks`.
typedef struct lca_memory {
  uint8_t *host_base;
  uint64_t host_size;
  const lca_region *regions;
  uint32_t region_count;
  uint32_t reserved;
  lca_callbacks callbacks;
} lca_memory;

// General-purpose registers X0 to X30 in lca_state.
#define LCA_GPR_COUNT 31

// The architectural state of one guest core. Register 31 is SP or the zero register depending
// on the instruction, so it is not in `x`. `nzcv` holds the flags in bits 31..28.
typedef struct lca_state {
  uint64_t x[LCA_GPR_COUNT];
  uint64_t sp;
  uint64_t pc;
  uint32_t nzcv;
  uint32_t reserved;
} lca_state;

// Why lca_run returned.
typedef uint32_t lca_exit_reason;
// An SVC completed: pc is the next instruction, `detail` the SVC number.
static const lca_exit_reason LCA_EXIT_SVC = 1;
// `limit` instructions ran: pc is the next instruction.
static const lca_exit_reason LCA_EXIT_LIMIT = 2;
// The word at pc is unallocated or reserved: nothing was executed for it; `address` is pc.
static const lca_exit_reason LCA_EXIT_UNDEFINED = 3;
// The table decodes the word at pc but the interpreter has no semantics for it yet: `detail`
// is its lca_insn, `address` is pc; nothing was executed for it.
static const lca_exit_reason LCA_EXIT_UNIMPLEMENTED = 4;
// pc is not in a region with execute permission: `address` is pc.
static const lca_exit_reason LCA_EXIT_FETCH_ABORT = 5;
// pc is not a multiple of 4 (PC alignment fault): `address` is pc; nothing was fetched.
static const lca_exit_reason LCA_EXIT_PC_MISALIGNED = 6;
// A data access failed on both the inline and the slow path: `address` is the guest address
// accessed, `detail` is LCA_PERM_READ or LCA_PERM_WRITE; pc is unchanged and the instruction
// changed nothing.
static const lca_exit_reason LCA_EXIT_DATA_ABORT = 7;

typedef struct lca_exit {
  lca_exit_reason reason;
  uint32_t detail;
  uint64_t address;
  // Instructions completed by this call (the deterministic clock of ADR-0004 counts these).
  uint64_t executed;
} lca_exit;

// Runs the core in `state` until an exit or until `limit` instructions completed. `state` is
// read on entry and written back on return. Every pointer is borrowed for the call only.
lca_status lca_run(lca_state *state, const lca_memory *memory, uint64_t limit,
                   lca_exit *out) LCA_NOEXCEPT;

#ifdef __cplusplus
}
#endif

#endif // LINCE_CPU_A64_H
