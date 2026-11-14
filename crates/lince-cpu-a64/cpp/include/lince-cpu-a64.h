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

#ifdef __cplusplus
}
#endif

#endif // LINCE_CPU_A64_H
