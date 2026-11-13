// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Safe wrapper over `cpp/include/boundary.h`. This module and its generated `bindings` are
//! the only places with `unsafe`, as ADR-0030 prescribes for `[c++]` crates (ARCH-24).
#![allow(
    unsafe_code,
    reason = "calls into the C++ library; see the SAFETY comments"
)]

mod bindings;

use std::ffi::c_void;
use std::fmt;

use bindings::{LB_DIVIDE_BY_ZERO, LB_NULL_ARGUMENT, LB_OK, LB_OVERFLOW, lb_status};

/// A non-`LB_OK` status returned by the C++ side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryError {
    /// A pointer argument was null; the wrapper never passes one, so this is a C++ bug.
    NullArgument,
    /// Division by zero.
    DivideByZero,
    /// The result does not fit in 32 bits.
    Overflow,
    /// A status this wrapper does not know; kept, not mapped to another variant (PAT-03).
    Unknown(i32),
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NullArgument => write!(f, "null argument passed to the C++ library"),
            Self::DivideByZero => write!(f, "division by zero"),
            Self::Overflow => write!(f, "result does not fit in 32 bits"),
            Self::Unknown(s) => write!(f, "unknown status {s} from the C++ library"),
        }
    }
}

impl std::error::Error for BoundaryError {}

fn check(status: lb_status) -> Result<(), BoundaryError> {
    match status {
        LB_OK => Ok(()),
        LB_NULL_ARGUMENT => Err(BoundaryError::NullArgument),
        LB_DIVIDE_BY_ZERO => Err(BoundaryError::DivideByZero),
        LB_OVERFLOW => Err(BoundaryError::Overflow),
        other => Err(BoundaryError::Unknown(other)),
    }
}

/// Guest memory as the C++ side sees it through the callback table.
pub trait Memory {
    /// The 32-bit word at `addr`.
    fn read_u32(&mut self, addr: u64) -> u32;
}

/// `a + b` (wrapping), computed in C++.
pub fn add(a: u32, b: u32) -> u32 {
    // SAFETY: plain values in, a plain value out; the function has no preconditions.
    unsafe { bindings::lb_add(a, b) }
}

/// Sum of `count` words at `base + i * stride`, read by C++ through `memory`.
pub fn sum_range<M: Memory>(
    memory: &mut M,
    base: u64,
    count: u32,
    stride: u32,
) -> Result<u64, BoundaryError> {
    let callbacks = bindings::lb_callbacks {
        ctx: std::ptr::from_mut(memory).cast::<c_void>(),
        read_u32: Some(read_trampoline::<M>),
    };
    let range = bindings::lb_range {
        base,
        count,
        stride,
    };
    let mut sum = 0;
    // SAFETY: `callbacks`, `range` and `sum` live until the call returns, and the header only
    // borrows them for the call (ARCH-53). `ctx` is `memory`, which this function borrows
    // mutably for the whole call, and `read_trampoline::<M>` casts it back to `M`.
    check(unsafe { bindings::lb_sum_range(&raw const callbacks, &raw const range, &raw mut sum) })?;
    Ok(sum)
}

/// `a / b` rounded toward zero, computed in C++.
pub fn checked_div(a: i32, b: i32) -> Result<i32, BoundaryError> {
    let mut quotient = 0;
    // SAFETY: `quotient` lives until the call returns; the function only writes through it.
    check(unsafe { bindings::lb_checked_div(a, b, &raw mut quotient) })?;
    Ok(quotient)
}

/// Called by C++ for each word. A panic here aborts instead of unwinding into C++, because
/// the function is `extern "C"` (ARCH-51).
extern "C" fn read_trampoline<M: Memory>(ctx: *mut c_void, addr: u64) -> u32 {
    // SAFETY: C++ passes back the `ctx` that `sum_range::<M>` put in the table, which points
    // to the `M` that `sum_range` borrows mutably for the duration of the call.
    let memory = unsafe { &mut *ctx.cast::<M>() };
    memory.read_u32(addr)
}

#[cfg(test)]
mod tests {
    use super::{BoundaryError, Memory, add, checked_div, sum_range};

    /// Word `i` holds `i * 3`; counts reads.
    struct Words {
        reads: u32,
    }

    impl Memory for Words {
        fn read_u32(&mut self, addr: u64) -> u32 {
            self.reads += 1;
            u32::try_from(addr / 4).expect("test addresses are small") * 3
        }
    }

    #[test]
    fn add_wraps_when_the_sum_overflows() {
        assert_eq!(add(2, 3), 5);
        assert_eq!(add(u32::MAX, 1), 0);
    }

    #[test]
    fn sum_range_reads_through_the_callback_once_per_word() {
        let mut memory = Words { reads: 0 };

        let sum = sum_range(&mut memory, 4, 5, 8);

        assert_eq!(sum, Ok(3 * (1 + 3 + 5 + 7 + 9)));
        assert_eq!(memory.reads, 5);
    }

    #[test]
    fn checked_div_reports_errors_as_statuses_when_the_division_is_invalid() {
        assert_eq!(checked_div(7, -2), Ok(-3));
        assert_eq!(checked_div(1, 0), Err(BoundaryError::DivideByZero));
        assert_eq!(checked_div(i32::MIN, -1), Err(BoundaryError::Overflow));
    }
}
