// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// PoC-11 C++ side. Built without exceptions or RTTI; every exported function is noexcept
// (ARCH-51). No threads, I/O or mutable globals (ARCH-52).

#include "boundary.h"

#include <cstdint>
#include <limits>

extern "C" {

uint32_t lb_add(uint32_t a, uint32_t b) noexcept { return a + b; }

lb_status lb_sum_range(const lb_callbacks *callbacks, const lb_range *range,
                       uint64_t *out) noexcept {
  if (callbacks == nullptr || callbacks->read_u32 == nullptr || range == nullptr ||
      out == nullptr) {
    return LB_NULL_ARGUMENT;
  }
  uint64_t sum = 0;
  uint64_t addr = range->base;
  for (uint32_t i = 0; i < range->count; ++i) {
    sum += callbacks->read_u32(callbacks->ctx, addr);
    addr += range->stride;
  }
  *out = sum;
  return LB_OK;
}

lb_status lb_checked_div(int32_t a, int32_t b, int32_t *out) noexcept {
  if (out == nullptr) {
    return LB_NULL_ARGUMENT;
  }
  if (b == 0) {
    return LB_DIVIDE_BY_ZERO;
  }
  // INT32_MIN / -1 does not fit in int32_t (undefined behavior in C++).
  if (a == std::numeric_limits<int32_t>::min() && b == -1) {
    return LB_OVERFLOW;
  }
  *out = a / b;
  return LB_OK;
}

} // extern "C"
