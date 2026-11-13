// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// C++ test driver for the sanitizer runs (`cargo xtask ci` builds it with ASan and UBSan).
// No test framework (ADR-0030): each check prints its name on failure and the exit status
// counts the failures. Behavior is also tested from Rust through the safe wrapper.

#include "boundary.h"

#include <array>
#include <cstdint>
#include <cstdio>
#include <limits>

namespace {

int failures = 0;

void check(bool ok, const char *name) {
  if (!ok) {
    std::printf("FAILED: %s\n", name);
    ++failures;
  }
}

// What the callback reads (word i holds i * 3) and how often it was called.
struct TestMemory {
  std::array<uint32_t, 16> words{};
  uint32_t reads = 0;
};

TestMemory make_memory() {
  TestMemory memory;
  for (uint32_t i = 0; i < memory.words.size(); ++i) {
    memory.words.at(i) = i * 3;
  }
  return memory;
}

uint32_t read_u32(void *ctx, uint64_t addr) {
  auto *memory = static_cast<TestMemory *>(ctx);
  ++memory->reads;
  return memory->words.at(addr / 4);
}

} // namespace

int main() {
  check(lb_add(2, 3) == 5, "lb_add adds");
  check(lb_add(std::numeric_limits<uint32_t>::max(), 1) == 0, "lb_add wraps");

  TestMemory memory = make_memory();
  const lb_callbacks callbacks{.ctx = &memory, .read_u32 = read_u32};
  const lb_range range{.base = 4, .count = 5, .stride = 8}; // words 1, 3, 5, 7, 9
  uint64_t sum = 0;
  check(lb_sum_range(&callbacks, &range, &sum) == LB_OK, "lb_sum_range succeeds");
  check(sum == uint64_t{3} * (1 + 3 + 5 + 7 + 9), "lb_sum_range sums through the callback");
  check(memory.reads == 5, "lb_sum_range calls back once per word");
  check(lb_sum_range(nullptr, &range, &sum) == LB_NULL_ARGUMENT, "lb_sum_range rejects null");

  int32_t q = 0;
  check(lb_checked_div(7, -2, &q) == LB_OK && q == -3, "lb_checked_div rounds toward zero");
  check(lb_checked_div(1, 0, &q) == LB_DIVIDE_BY_ZERO, "lb_checked_div reports zero");
  check(lb_checked_div(std::numeric_limits<int32_t>::min(), -1, &q) == LB_OVERFLOW,
        "lb_checked_div reports overflow");

  if (failures == 0) {
    std::printf("boundary_test: all checks passed\n");
  }
  return failures;
}
