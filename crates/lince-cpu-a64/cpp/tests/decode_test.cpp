// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// C++ test driver for the sanitizer runs (`cargo xtask ci` builds it with ASan and UBSan).
// No test framework (ADR-0030): each check prints its name on failure and the exit status
// counts the failures. Decoding is tested in depth from Rust (tests/decode.rs).

#include "lince-cpu-a64.h"

#include <array>
#include <cstdint>
#include <cstdio>
#include <limits>
#include <string_view>

namespace {

int failures = 0;

void check(bool ok, const char *name) {
  if (!ok) {
    std::printf("FAILED: %s\n", name);
    ++failures;
  }
}

std::string_view name_of(lca_insn insn, std::array<char, LCA_INSN_NAME_MAX> &buffer) {
  const uint32_t length = lca_insn_name(insn, buffer.data());
  return {buffer.data(), length};
}

} // namespace

int main() {
  std::array<char, LCA_INSN_NAME_MAX> buffer{};
  const uint32_t ret_x30 = 0xd65f03c0; // ret
  check(name_of(lca_decode(ret_x30), buffer) == "RET", "lca_decode finds RET");
  check(lca_decode(0) == LCA_INSN_UNKNOWN, "lca_decode reports udf #0 as unknown");
  check(lca_insn_name(LCA_INSN_UNKNOWN, buffer.data()) == 0, "UNKNOWN has no name");
  check(lca_insn_name(std::numeric_limits<lca_insn>::max(), buffer.data()) == 0,
        "an out-of-range id has no name");
  check(lca_insn_name(lca_decode(ret_x30), nullptr) == 0, "a null buffer is rejected");

  // Every word decodes without touching memory out of bounds (the sanitizers check this);
  // a stride that is odd and large visits words with every bit pattern position.
  const uint32_t stride = 0x10001;
  uint32_t known = 0;
  for (uint64_t word = 0; word <= std::numeric_limits<uint32_t>::max(); word += stride) {
    const lca_insn insn = lca_decode(static_cast<uint32_t>(word));
    if (insn != LCA_INSN_UNKNOWN) {
      ++known;
      check(!name_of(insn, buffer).empty(), "every decoded id has a name");
    }
  }
  check(known > 0, "the sweep decodes some words");

  if (failures == 0) {
    std::printf("decode_test: all checks passed\n");
  }
  return failures;
}
