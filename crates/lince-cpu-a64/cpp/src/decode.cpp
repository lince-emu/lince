// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The decoder, generated at compile time from insns.inc: each pattern becomes a mask/value
// line, and static_asserts reject a malformed pattern or two patterns that match the same
// word, so the table itself cannot be ambiguous. Built without exceptions or RTTI; no
// threads, I/O or mutable globals (ARCH-51, ARCH-52).

#include "decode.hpp"

#include "lince-cpu-a64.h"

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <string_view>
#include <type_traits>

namespace lince::a64 {
namespace {

constexpr std::size_t kMaxPatterns = 4;
constexpr std::size_t kWordBits = 32;

// One LCA_INSN entry as written in the table.
struct Entry {
  std::string_view name;
  std::array<std::string_view, kMaxPatterns> patterns;
};

constexpr std::array kEntries = {
    Entry{.name = "UNKNOWN", .patterns = {}},
#define LCA_INSN(id, ...) Entry{.name = #id, .patterns = {__VA_ARGS__}},
#include "insns.inc"
#undef LCA_INSN
};

// One pattern as a test: `(word & mask) == value`.
struct Line {
  uint32_t mask = 0;
  uint32_t value = 0;
  uint16_t insn = 0;
  bool well_formed = false;
};

// A pattern is well formed when it has exactly 32 bits of '0', '1' or 'x' (spaces ignored).
constexpr Line parse(std::string_view pattern, uint16_t insn) {
  Line line{.insn = insn, .well_formed = true};
  std::size_t bits = 0;
  for (const char c : pattern) {
    if (c == ' ') {
      continue;
    }
    if (c != '0' && c != '1' && c != 'x') {
      line.well_formed = false;
    }
    line.mask = (line.mask << 1U) | (c == 'x' ? 0U : 1U);
    line.value = (line.value << 1U) | (c == '1' ? 1U : 0U);
    ++bits;
  }
  line.well_formed = line.well_formed && bits == kWordBits;
  return line;
}

constexpr std::size_t count_lines() {
  std::size_t count = 0;
  for (const Entry &entry : kEntries) {
    count += static_cast<std::size_t>(std::ranges::count_if(
        entry.patterns, [](std::string_view pattern) { return !pattern.empty(); }));
  }
  return count;
}

constexpr std::array<Line, count_lines()> build_lines() {
  std::array<Line, count_lines()> lines{};
  std::size_t next = 0;
  for (std::size_t insn = 0; insn < kEntries.size(); ++insn) {
    for (const std::string_view pattern : kEntries.at(insn).patterns) {
      if (!pattern.empty()) {
        lines.at(next) = parse(pattern, static_cast<uint16_t>(insn));
        ++next;
      }
    }
  }
  return lines;
}

constexpr auto kLines = build_lines();

constexpr bool all_well_formed() {
  return std::ranges::all_of(kLines, [](const Line &line) { return line.well_formed; });
}

// Two lines overlap when some word satisfies both: their values agree on every bit that both
// of them fix.
constexpr bool none_overlap() {
  for (std::size_t i = 0; i < kLines.size(); ++i) {
    for (std::size_t j = i + 1; j < kLines.size(); ++j) {
      const Line &a = kLines.at(i);
      const Line &b = kLines.at(j);
      if (((a.value ^ b.value) & a.mask & b.mask) == 0) {
        return false;
      }
    }
  }
  return true;
}

constexpr bool names_fit() {
  return std::ranges::all_of(
      kEntries, [](const Entry &entry) { return entry.name.size() <= LCA_INSN_NAME_MAX; });
}

static_assert(std::is_same_v<std::underlying_type_t<Insn>, lca_insn>,
              "Insn and lca_insn must have the same representation");
static_assert(kEntries.size() <= std::numeric_limits<lca_insn>::max(),
              "too many instructions for lca_insn");
static_assert(all_well_formed(), "an insns.inc pattern does not have 32 bits of 0, 1 or x");
static_assert(none_overlap(), "two insns.inc patterns match the same word");
static_assert(names_fit(), "an insns.inc name is longer than LCA_INSN_NAME_MAX");

} // namespace

Insn decode(uint32_t word) noexcept {
  for (const Line &line : kLines) {
    if ((word & line.mask) == line.value) {
      return static_cast<Insn>(line.insn);
    }
  }
  return Insn::UNKNOWN;
}

std::string_view name(Insn insn) noexcept {
  const auto index = static_cast<std::size_t>(insn);
  if (insn == Insn::UNKNOWN || index >= kEntries.size()) {
    return {};
  }
  return kEntries.at(index).name;
}

} // namespace lince::a64

extern "C" {

lca_insn lca_decode(uint32_t word) noexcept {
  return static_cast<lca_insn>(lince::a64::decode(word));
}

uint32_t lca_insn_name(lca_insn insn, char *out) noexcept {
  if (out == nullptr) {
    return 0;
  }
  const std::string_view name = lince::a64::name(static_cast<lince::a64::Insn>(insn));
  std::ranges::copy(name, out);
  return static_cast<uint32_t>(name.size());
}

} // extern "C"
