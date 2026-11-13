// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// PoC-11: the C header that is the whole Rust/C++ contract (ADR-0030, ARCH-50).
//
// Only extern "C" functions, plain structs of fixed-size fields and opaque pointers cross
// the boundary. Every function is noexcept on the C++ side and reports errors as a status.
// Ownership (ARCH-53): every pointer argument is owned by the caller and is only borrowed for
// the duration of the call; nothing is allocated on one side and freed on the other.

#ifndef LINCE_POC_BOUNDARY_H
#define LINCE_POC_BOUNDARY_H

#include <stdint.h>

#ifdef __cplusplus
#define LB_NOEXCEPT noexcept
extern "C" {
#else
#define LB_NOEXCEPT
#endif

// Status codes. A fixed-width integer, not an enum, so its size is the same in C, C++ and Rust.
typedef int32_t lb_status;
static const lb_status LB_OK = 0;
static const lb_status LB_NULL_ARGUMENT = 1;
static const lb_status LB_DIVIDE_BY_ZERO = 2;
static const lb_status LB_OVERFLOW = 3;

// Services C++ calls back into Rust (memory access in a real component). `ctx` is passed back
// unchanged. The callbacks must not unwind (ARCH-51).
typedef struct lb_callbacks {
  void *ctx;
  uint32_t (*read_u32)(void *ctx, uint64_t addr);
} lb_callbacks;

// A strided range of 32-bit words.
typedef struct lb_range {
  uint64_t base;
  uint32_t count;
  uint32_t stride;
} lb_range;

// The cheapest possible call: measures the Rust -> C++ call cost.
uint32_t lb_add(uint32_t a, uint32_t b) LB_NOEXCEPT;

// Sums `range->count` words read through `callbacks->read_u32` at `base + i * stride`:
// measures the C++ -> Rust callback cost. Writes the sum to `out`.
lb_status lb_sum_range(const lb_callbacks *callbacks, const lb_range *range,
                       uint64_t *out) LB_NOEXCEPT;

// `a / b` rounded toward zero, as an example of an error crossing as a status.
lb_status lb_checked_div(int32_t a, int32_t b, int32_t *out) LB_NOEXCEPT;

#ifdef __cplusplus
}
#endif

#endif // LINCE_POC_BOUNDARY_H
