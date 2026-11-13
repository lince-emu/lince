// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Address newtypes for the guest CPU and guest GPU address spaces (PAT-02, QUAL-20).
//!
//! Distinct types keep a CPU address from being passed where a GPU address is expected.
//! There is no `Deref` or `Add`: arithmetic goes through methods that say how they overflow.
//!
//! Address widths are not checked: the Switch 2 values are UNKNOWN (doc 08), and range checks
//! belong to the memory crates that own each space.

use std::fmt;

/// A guest CPU virtual address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct GuestVa(u64);

impl GuestVa {
    /// Wraps a raw address read at a boundary (register, descriptor, syscall).
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value, for writing back at a boundary.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// `None` if the result passes the end of the address space.
    #[must_use]
    pub const fn checked_add(self, bytes: u64) -> Option<Self> {
        match self.0.checked_add(bytes) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Wraps at the end of the space, as guest pointer arithmetic does.
    #[must_use]
    pub const fn wrapping_add(self, bytes: u64) -> Self {
        Self(self.0.wrapping_add(bytes))
    }

    /// Bytes from `base` to `self`; `None` if `self` is below `base`.
    pub const fn checked_offset_from(self, base: Self) -> Option<u64> {
        self.0.checked_sub(base.0)
    }

    /// `align` must be a power of two.
    pub const fn is_aligned_to(self, align: u64) -> bool {
        debug_assert!(align.is_power_of_two(), "alignment must be a power of two");
        self.0 & (align - 1) == 0
    }
}

impl fmt::LowerHex for GuestVa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

/// A guest GPU virtual address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct GpuVa(u64);

impl GpuVa {
    /// Wraps a raw address read at a boundary (method argument, descriptor).
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value, for writing back at a boundary.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// `None` if the result passes the end of the address space.
    #[must_use]
    pub const fn checked_add(self, bytes: u64) -> Option<Self> {
        match self.0.checked_add(bytes) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    /// Bytes from `base` to `self`; `None` if `self` is below `base`.
    pub const fn checked_offset_from(self, base: Self) -> Option<u64> {
        self.0.checked_sub(base.0)
    }

    /// `align` must be a power of two.
    pub const fn is_aligned_to(self, align: u64) -> bool {
        debug_assert!(align.is_power_of_two(), "alignment must be a power of two");
        self.0 & (align - 1) == 0
    }
}

impl fmt::LowerHex for GpuVa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::{GpuVa, GuestVa};

    #[test]
    fn checked_add_returns_none_when_the_address_space_ends() {
        let last = GuestVa::new(u64::MAX - 1);

        let next = last.checked_add(1);
        let past = last.checked_add(2);

        assert_eq!(next, Some(GuestVa::new(u64::MAX)));
        assert_eq!(past, None);
    }

    #[test]
    fn wrapping_add_wraps_when_the_address_space_ends() {
        let last = GuestVa::new(u64::MAX);

        let wrapped = last.wrapping_add(2);

        assert_eq!(wrapped, GuestVa::new(1));
    }

    #[test]
    fn checked_offset_from_returns_none_when_the_base_is_higher() {
        let base = GpuVa::new(0x1000);

        let inside = GpuVa::new(0x1040).checked_offset_from(base);
        let below = GpuVa::new(0x0FC0).checked_offset_from(base);

        assert_eq!(inside, Some(0x40));
        assert_eq!(below, None);
    }

    #[test]
    fn is_aligned_to_is_true_only_when_the_low_bits_are_zero() {
        let page = GuestVa::new(0x4000);
        let odd = GuestVa::new(0x4004);

        assert!(page.is_aligned_to(0x1000));
        assert!(odd.is_aligned_to(4));
        assert!(!odd.is_aligned_to(8));
    }

    #[test]
    fn lower_hex_formats_the_raw_address_when_logged() {
        let va = GpuVa::new(0xdead_beef);

        let text = format!("{va:#x}");

        assert_eq!(text, "0xdeadbeef");
    }
}
