// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The guest address space: regions with permissions over one host reservation.
//!
//! Guest address `va` lives at host address `base + va`. Host memory is committed in units of
//! the configured host page size, which may be larger than the 4 KiB guest page: a host page
//! stays committed while any guest page inside it is mapped. Permissions are checked in
//! software on every access (ADR-0019 B1 scope); host protection is only "committed or not".

use std::collections::BTreeMap;
use std::ops::BitOr;
use std::ptr;

use lince_common::GuestVa;

use crate::error::{Access, MemError, Result};
use crate::platform::{self, Reservation};

/// Size of a guest page in bytes.
pub const GUEST_PAGE_SIZE: u64 = 0x1000;

/// Guest page permissions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permission(u8);

impl Permission {
    /// No access.
    pub const NONE: Self = Self(0);
    /// Loads.
    pub const READ: Self = Self(1);
    /// Stores.
    pub const WRITE: Self = Self(2);
    /// Instruction fetch.
    pub const EXECUTE: Self = Self(4);

    /// The permissions of both `self` and `other`; `const` form of `|`.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every permission in `other` is also in `self`.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    const fn allows(self, access: Access) -> bool {
        match access {
            Access::Read => self.contains(Self::READ),
            Access::Write => self.contains(Self::WRITE),
        }
    }
}

impl BitOr for Permission {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

/// How the guest space is laid out on the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpaceConfig {
    /// Width of the guest address space; Switch 2 titles use 39 (doc 01 E2).
    pub address_bits: u32,
    /// Host page size used to commit memory: a power of two, at least the guest page and a
    /// multiple of the OS page size. Larger values model hosts with 16 KiB pages.
    pub host_page_size: u64,
}

impl SpaceConfig {
    /// A 39-bit space using this host's page size.
    pub fn for_host() -> Self {
        Self {
            address_bits: 39,
            host_page_size: platform::page_size() as u64,
        }
    }
}

/// A guest value read or written as little-endian bytes. Implemented for `u8`–`u64`.
pub trait Scalar: Copy + sealed::Sealed {
    /// The byte array of the value.
    type Bytes: AsRef<[u8]> + AsMut<[u8]> + Default;
    /// Builds a value from guest bytes.
    fn from_le(bytes: Self::Bytes) -> Self;
    /// The guest bytes of the value.
    fn to_le(self) -> Self::Bytes;
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! scalar {
    ($($t:ty),*) => {$(
        impl sealed::Sealed for $t {}
        impl Scalar for $t {
            type Bytes = [u8; size_of::<$t>()];
            fn from_le(bytes: Self::Bytes) -> Self {
                <$t>::from_le_bytes(bytes)
            }
            fn to_le(self) -> Self::Bytes {
                self.to_le_bytes()
            }
        }
    )*};
}
scalar!(u8, u16, u32, u64);

#[derive(Clone, Copy, Debug)]
struct Region {
    end: u64,
    perm: Permission,
}

/// A guest address space.
pub struct AddressSpace {
    host: Reservation,
    size: u64,
    host_page: u64,
    /// Mapped regions by start address; they never overlap.
    regions: BTreeMap<u64, Region>,
}

impl AddressSpace {
    /// Reserves host address space for the whole guest space; nothing is mapped yet.
    pub fn new(config: SpaceConfig) -> Result<Self> {
        let invalid = |reason| MemError::InvalidConfig { reason };
        if !(12..=47).contains(&config.address_bits) {
            return Err(invalid("address_bits must be in 12..=47"));
        }
        let host_page = config.host_page_size;
        let os_page = platform::page_size() as u64;
        if !host_page.is_power_of_two() || host_page < GUEST_PAGE_SIZE || host_page < os_page {
            return Err(invalid(
                "host_page_size must be a power of two no smaller than the guest and OS pages",
            ));
        }
        let size = 1u64 << config.address_bits;
        if host_page > size {
            return Err(invalid("host_page_size is larger than the address space"));
        }
        let len = usize::try_from(size).map_err(|_| invalid("address space too large for host"))?;
        Ok(Self {
            host: Reservation::new(len)?,
            size,
            host_page,
            regions: BTreeMap::new(),
        })
    }

    /// Maps `[va, va + size)` with `perm`, zero-filled. The range must be page-aligned,
    /// inside the space and free.
    pub fn map(&mut self, va: GuestVa, size: u64, perm: Permission) -> Result<()> {
        let end = self.checked_range(va, size)?;
        if self.overlaps(va.get(), end) {
            return Err(MemError::Overlap { va, size });
        }
        let lo = round_down(va.get(), self.host_page);
        let hi = round_up(end, self.host_page);
        self.host.commit(to_usize(lo), to_usize(hi - lo))?;
        // SAFETY: `[va, end)` lies inside the reservation (`checked_range`) and was committed
        // read-write above; no Rust reference into guest memory exists.
        unsafe {
            ptr::write_bytes(
                self.host.base().as_ptr().add(to_usize(va.get())),
                0,
                to_usize(size),
            );
        }
        self.regions.insert(va.get(), Region { end, perm });
        Ok(())
    }

    /// Unmaps the region that starts at `va` and is `size` bytes long.
    pub fn unmap(&mut self, va: GuestVa, size: u64) -> Result<()> {
        let end = self.checked_range(va, size)?;
        let removed = match self.regions.get(&va.get()) {
            Some(region) if region.end == end => *region,
            _ => return Err(MemError::NotARegion { va, size }),
        };
        self.regions.remove(&va.get());
        // Host pages at either edge may still hold other mapped guest pages.
        let mut lo = round_down(va.get(), self.host_page);
        let mut hi = round_up(end, self.host_page);
        if self.overlaps(lo, lo + self.host_page) {
            lo += self.host_page;
        }
        if hi > lo && self.overlaps(hi - self.host_page, hi) {
            hi -= self.host_page;
        }
        if hi > lo
            && let Err(error) = self.host.decommit(to_usize(lo), to_usize(hi - lo))
        {
            self.regions.insert(va.get(), removed);
            return Err(error.into());
        }
        Ok(())
    }

    /// Sets the permission of the region that starts at `va` and is `size` bytes long.
    pub fn protect(&mut self, va: GuestVa, size: u64, perm: Permission) -> Result<()> {
        let end = self.checked_range(va, size)?;
        match self.regions.get_mut(&va.get()) {
            Some(region) if region.end == end => {
                region.perm = perm;
                Ok(())
            }
            _ => Err(MemError::NotARegion { va, size }),
        }
    }

    /// The permission of the region containing `va`, or `None` if it is unmapped.
    pub fn permission(&self, va: GuestVa) -> Option<Permission> {
        self.region_at(va.get()).map(|(_, region)| region.perm)
    }

    /// Reads a little-endian value at `va`.
    pub fn read<T: Scalar>(&self, va: GuestVa) -> Result<T> {
        let mut bytes = T::Bytes::default();
        self.read_bytes(va, bytes.as_mut())?;
        Ok(T::from_le(bytes))
    }

    /// Writes a little-endian value at `va`.
    pub fn write<T: Scalar>(&mut self, va: GuestVa, value: T) -> Result<()> {
        self.write_bytes(va, value.to_le().as_ref())
    }

    /// Fills `buf` from guest memory at `va`; every byte must be mapped and readable.
    pub fn read_bytes(&self, va: GuestVa, buf: &mut [u8]) -> Result<()> {
        let offset = self.check_access(va, buf.len(), Access::Read)?;
        // SAFETY: `check_access` proved the range is mapped, hence committed read-write and
        // inside the reservation; `buf` is a separate Rust allocation, so they cannot overlap.
        unsafe {
            ptr::copy_nonoverlapping(
                self.host.base().as_ptr().add(offset),
                buf.as_mut_ptr(),
                buf.len(),
            );
        }
        Ok(())
    }

    /// Copies `data` to guest memory at `va`; every byte must be mapped and writable.
    pub fn write_bytes(&mut self, va: GuestVa, data: &[u8]) -> Result<()> {
        let offset = self.check_access(va, data.len(), Access::Write)?;
        // SAFETY: as in `read_bytes`, with the destination in guest memory.
        unsafe {
            ptr::copy_nonoverlapping(
                data.as_ptr(),
                self.host.base().as_ptr().add(offset),
                data.len(),
            );
        }
        Ok(())
    }

    /// Checks that `[va, va + len)` is mapped with `access` allowed; returns the host offset.
    fn check_access(&self, va: GuestVa, len: usize, access: Access) -> Result<usize> {
        let end = va
            .get()
            .checked_add(len as u64)
            .filter(|end| *end <= self.size)
            .ok_or(MemError::Unmapped { va, access })?;
        let mut addr = va.get();
        while addr < end {
            let Some((_, region)) = self.region_at(addr) else {
                return Err(MemError::Unmapped {
                    va: GuestVa::new(addr),
                    access,
                });
            };
            if !region.perm.allows(access) {
                return Err(MemError::PermissionDenied {
                    va: GuestVa::new(addr),
                    access,
                });
            }
            addr = region.end;
        }
        Ok(to_usize(va.get()))
    }

    fn checked_range(&self, va: GuestVa, size: u64) -> Result<u64> {
        if size == 0 || !va.is_aligned_to(GUEST_PAGE_SIZE) || !size.is_multiple_of(GUEST_PAGE_SIZE)
        {
            return Err(MemError::Misaligned { va, size });
        }
        va.get()
            .checked_add(size)
            .filter(|end| *end <= self.size)
            .ok_or(MemError::OutOfRange { va, size })
    }

    fn region_at(&self, addr: u64) -> Option<(u64, Region)> {
        let (&start, &region) = self.regions.range(..=addr).next_back()?;
        (addr < region.end).then_some((start, region))
    }

    fn overlaps(&self, start: u64, end: u64) -> bool {
        self.regions
            .range(..end)
            .next_back()
            .is_some_and(|(_, region)| region.end > start)
    }
}

fn round_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

fn round_up(value: u64, align: u64) -> u64 {
    round_down(value + align - 1, align)
}

/// Offsets are bounded by the reservation length, which is a `usize`.
fn to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}
