// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The A64 decoder shared by every execution tier (ADR-0004), generated at compile time from
// the description table in insns.inc.

#ifndef LINCE_CPU_A64_DECODE_HPP
#define LINCE_CPU_A64_DECODE_HPP

#include <cstdint>
#include <string_view>

namespace lince::a64 {

// One id per LCA_INSN entry, in table order; the C API's lca_insn carries the same values.
enum class Insn : uint16_t {
  UNKNOWN = 0,
#define LCA_INSN(id, ...) id,
#include "insns.inc"
#undef LCA_INSN
};

// The instruction form of `word`, or Insn::UNKNOWN when no table entry matches.
Insn decode(uint32_t word) noexcept;

// The table name of `insn`; empty for Insn::UNKNOWN and for values outside the table.
std::string_view name(Insn insn) noexcept;

} // namespace lince::a64

#endif // LINCE_CPU_A64_DECODE_HPP
