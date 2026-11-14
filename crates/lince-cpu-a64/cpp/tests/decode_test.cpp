// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// C++ test driver for the sanitizer runs (`cargo xtask ci` builds it with ASan and UBSan).
// No test framework (ADR-0030): each check prints its name on failure and the exit status
// counts the failures. Decoding and execution are tested in depth from Rust (tests/*.rs).

#include "lince-cpu-a64.h"

#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
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

// A two-instruction program in a buffer twice its size that stands in for the host
// reservation; only the program is mapped.
void check_run() {
  constexpr uint32_t kMovX1 = 0xd2800541; // mov x1, #42
  constexpr uint64_t kMovedValue = 42;
  constexpr uint32_t kSvc = 0xd40000e1; // svc #0x7
  constexpr uint32_t kSvcNumber = 7;
  constexpr uint64_t kLimit = 10;
  const std::array<uint32_t, 2> program{kMovX1, kSvc};
  std::array<uint8_t, 2 * sizeof program> host{};
  std::memcpy(host.data(), program.data(), sizeof program);
  const lca_region code{
      .base = 0,
      .size = sizeof program,
      .perm = LCA_PERM_READ | LCA_PERM_EXECUTE,
      .reserved = 0,
  };
  const lca_memory memory{
      .host_base = host.data(),
      .host_size = host.size(),
      .regions = &code,
      .region_count = 1,
      .reserved = 0,
      .callbacks = {.context = nullptr, .read = nullptr, .write = nullptr},
  };
  lca_state state{};
  lca_exit out{};

  check(lca_run(&state, &memory, kLimit, &out) == LCA_OK, "lca_run succeeds");
  check(out.reason == LCA_EXIT_SVC && out.detail == kSvcNumber && out.executed == 2,
        "lca_run exits on SVC");
  check(state.x[1] == kMovedValue && state.pc == sizeof program, "lca_run writes the state back");

  check(lca_run(&state, &memory, kLimit, &out) == LCA_OK && out.reason == LCA_EXIT_FETCH_ABORT,
        "lca_run aborts when pc leaves the region");
  constexpr uint32_t kLdrX0X1 = 0xf9400020; // ldr x0, [x1]
  constexpr uint64_t kOutside = 0x1000;
  std::memcpy(host.data(), &kLdrX0X1, sizeof kLdrX0X1);
  state = lca_state{};
  state.x[1] = kOutside;
  check(lca_run(&state, &memory, kLimit, &out) == LCA_OK && out.reason == LCA_EXIT_DATA_ABORT &&
            out.address == kOutside && out.detail == LCA_PERM_READ && state.pc == 0,
        "a load outside the regions without callbacks is a data abort");
  check(lca_run(nullptr, &memory, 1, &out) == LCA_NULL_ARGUMENT, "lca_run rejects null state");
  check(lca_run(&state, nullptr, 1, &out) == LCA_NULL_ARGUMENT, "lca_run rejects null memory");
  check(lca_run(&state, &memory, 1, nullptr) == LCA_NULL_ARGUMENT, "lca_run rejects null exit");
}

} // namespace

int main() {
  check_run();
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
