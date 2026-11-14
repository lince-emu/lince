// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The guest address space: regions with permissions over one host reservation.
//!
//! Guest address `va` lives at host address `base + va`. Host memory is committed in units of
//! the configured host page size, which may be larger than the 4 KiB guest page: a host page
//! stays committed while any guest page inside it is mapped. Permissions are checked in
//! software on every access (ADR-0019 B1 scope); host protection is only "committed or not".
//!
//! The core threads of a process share one space (ADR-0019 B2 scope). The region map sits
//! behind a reader–writer lock: accesses and queries take the read side, `map`, `unmap` and
//! `protect` the write side, so no thread sees a half-updated map. Page contents are not
//! locked: Rust copies them with relaxed atomic byte accesses, and guest-ordered accesses use
//! host atomics on the bytes (ADR-0004).

use std::collections::BTreeMap;
use std::ops::BitOr;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicU8, Ordering};

use lince_common::GuestVa;
use parking_lot::{RwLock, RwLockReadGuard};

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

    /// The permission bits: read = 1, write = 2, execute = 4, as in `lince-cpu-a64`'s C header.
    pub const fn bits(self) -> u8 {
        self.0
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

/// Mapped regions by start address; they never overlap.
#[derive(Default)]
struct Regions(BTreeMap<u64, Region>);

/// A maximal span of the address space with one state: a mapped region or a free gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// First address of the span.
    pub base: GuestVa,
    /// Length of the span in bytes.
    pub size: u64,
    /// The permission of a mapped region; `None` for a free gap.
    pub permission: Option<Permission>,
}

/// A guest address space, shared by the core threads of one process (`Send + Sync`).
pub struct AddressSpace {
    host: Reservation,
    size: u64,
    host_page: u64,
    regions: RwLock<Regions>,
}

/// The region map of an [`AddressSpace`], locked for reading.
///
/// While a view lives no region is mapped, unmapped or reprotected, so code that accesses guest
/// memory inline through [`host_base`](Self::host_base) can trust [`regions`](Self::regions)
/// (ADR-0031). `map`, `unmap` and `protect` on any thread wait until every view is dropped, so
/// hold one for a bounded time. A thread that holds a view must reach the space only through
/// it: taking a second view, or calling an [`AddressSpace`] method, may deadlock.
pub struct SpaceView<'a> {
    space: &'a AddressSpace,
    regions: RwLockReadGuard<'a, Regions>,
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
            regions: RwLock::new(Regions::default()),
        })
    }

    /// Locks the region map for reading until the view is dropped.
    pub fn view(&self) -> SpaceView<'_> {
        SpaceView {
            space: self,
            regions: self.regions.read(),
        }
    }

    /// Maps `[va, va + size)` with `perm`, zero-filled. The range must be page-aligned,
    /// inside the space and free.
    pub fn map(&self, va: GuestVa, size: u64, perm: Permission) -> Result<()> {
        let end = self.checked_range(va, size)?;
        let mut regions = self.regions.write();
        if regions.overlaps(va.get(), end) {
            return Err(MemError::Overlap { va, size });
        }
        let lo = round_down(va.get(), self.host_page);
        let hi = round_up(end, self.host_page);
        self.host.commit(to_usize(lo), to_usize(hi - lo))?;
        // The zero-fill runs under the write lock, so the range is never reachable unzeroed;
        // cores wait for it in `view`. Accepted cost; shrink the section if the ledger shows it.
        // SAFETY: `[va, end)` lies inside the reservation (`checked_range`) and was committed
        // read-write above. It was free until now, and the write lock excludes every view, so
        // no other thread accesses these bytes.
        unsafe {
            ptr::write_bytes(
                self.host.base().as_ptr().add(to_usize(va.get())),
                0,
                to_usize(size),
            );
        }
        regions.0.insert(va.get(), Region { end, perm });
        Ok(())
    }

    /// Unmaps `[va, va + size)`, which must be page-aligned and fully mapped; it may cover
    /// several regions or part of one. A region that extends past either end keeps its outer
    /// part.
    pub fn unmap(&self, va: GuestVa, size: u64) -> Result<()> {
        let mut regions = self.regions.write();
        let end = self.mapped_range(&regions, va, size)?;
        regions.split_at(va.get());
        regions.split_at(end);
        let mut removed = regions.0.split_off(&va.get());
        let mut after = removed.split_off(&end);
        regions.0.append(&mut after);
        // Host pages at either edge may still hold other mapped guest pages.
        let mut lo = round_down(va.get(), self.host_page);
        let mut hi = round_up(end, self.host_page);
        if regions.overlaps(lo, lo + self.host_page) {
            lo += self.host_page;
        }
        if hi > lo && regions.overlaps(hi - self.host_page, hi) {
            hi -= self.host_page;
        }
        if hi > lo
            && let Err(error) = self.host.decommit(to_usize(lo), to_usize(hi - lo))
        {
            regions.0.append(&mut removed);
            regions.merge(va.get(), end);
            return Err(error.into());
        }
        Ok(())
    }

    /// Sets the permission of `[va, va + size)`, which must be page-aligned and fully mapped;
    /// it may cover several regions or part of one. Regions are split at the range's ends, and
    /// neighbours that end up with the same permission are merged, so the region map stays the
    /// smallest one that describes the space.
    pub fn protect(&self, va: GuestVa, size: u64, perm: Permission) -> Result<()> {
        let mut regions = self.regions.write();
        let end = self.mapped_range(&regions, va, size)?;
        regions.split_at(va.get());
        regions.split_at(end);
        for (_, region) in regions.0.range_mut(va.get()..end) {
            region.perm = perm;
        }
        regions.merge(va.get(), end);
        Ok(())
    }

    /// The permission of the region containing `va`, or `None` if it is unmapped.
    pub fn permission(&self, va: GuestVa) -> Option<Permission> {
        self.view().permission(va)
    }

    /// The span containing `va`: its mapped region, or the free gap around it. `None` if `va`
    /// is outside the address space.
    pub fn query(&self, va: GuestVa) -> Option<Span> {
        self.view().query(va)
    }

    /// Size of the address space in bytes; every valid address is below it.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Reads a little-endian value at `va`.
    pub fn read<T: Scalar>(&self, va: GuestVa) -> Result<T> {
        self.view().read(va)
    }

    /// Writes a little-endian value at `va`.
    pub fn write<T: Scalar>(&self, va: GuestVa, value: T) -> Result<()> {
        self.view().write(va, value)
    }

    /// Fills `buf` from guest memory at `va`; every byte must be mapped and readable.
    pub fn read_bytes(&self, va: GuestVa, buf: &mut [u8]) -> Result<()> {
        self.view().read_bytes(va, buf)
    }

    /// Copies `data` to guest memory at `va`; every byte must be mapped and writable.
    pub fn write_bytes(&self, va: GuestVa, data: &[u8]) -> Result<()> {
        self.view().write_bytes(va, data)
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

    /// Checks that `[va, va + size)` is page-aligned, inside the space and mapped with no gap;
    /// returns its end.
    fn mapped_range(&self, regions: &Regions, va: GuestVa, size: u64) -> Result<u64> {
        let end = self.checked_range(va, size)?;
        let mut addr = va.get();
        while addr < end {
            let (_, region) = regions
                .region_at(addr)
                .ok_or(MemError::NotARegion { va, size })?;
            addr = region.end;
        }
        Ok(end)
    }
}

impl SpaceView<'_> {
    /// The permission of the region containing `va`, or `None` if it is unmapped.
    pub fn permission(&self, va: GuestVa) -> Option<Permission> {
        self.regions
            .region_at(va.get())
            .map(|(_, region)| region.perm)
    }

    /// The span containing `va`: its mapped region, or the free gap around it. `None` if `va`
    /// is outside the address space.
    pub fn query(&self, va: GuestVa) -> Option<Span> {
        let addr = va.get();
        if addr >= self.space.size {
            return None;
        }
        if let Some((start, region)) = self.regions.region_at(addr) {
            return Some(Span {
                base: GuestVa::new(start),
                size: region.end - start,
                permission: Some(region.perm),
            });
        }
        let base = self
            .regions
            .0
            .range(..addr)
            .next_back()
            .map_or(0, |(_, region)| region.end);
        let end = self
            .regions
            .0
            .range(addr..)
            .next()
            .map_or(self.space.size, |(&start, _)| start);
        Some(Span {
            base: GuestVa::new(base),
            size: end - base,
            permission: None,
        })
    }

    /// Size of the address space in bytes; every valid address is below it.
    pub fn size(&self) -> u64 {
        self.space.size
    }

    /// Every mapped region, in address order.
    pub fn regions(&self) -> impl Iterator<Item = Span> + '_ {
        self.regions.0.iter().map(|(&start, region)| Span {
            base: GuestVa::new(start),
            size: region.end - start,
            permission: Some(region.perm),
        })
    }

    /// The host address of guest address 0, for code that accesses guest memory inline (the
    /// interpreter and JITs, ADR-0031).
    ///
    /// Guest byte `va` of a region listed by [`regions`](Self::regions) is at `host_base() + va`,
    /// committed and host-writable for as long as this view lives. Every other address may be
    /// uncommitted. Permissions are not enforced by the host: whoever uses the pointer checks
    /// them against `regions`. Other threads may access the same bytes at the same time.
    pub fn host_base(&self) -> NonNull<u8> {
        self.space.host.base()
    }

    /// Reads a little-endian value at `va`.
    pub fn read<T: Scalar>(&self, va: GuestVa) -> Result<T> {
        let mut bytes = T::Bytes::default();
        self.read_bytes(va, bytes.as_mut())?;
        Ok(T::from_le(bytes))
    }

    /// Writes a little-endian value at `va`.
    pub fn write<T: Scalar>(&self, va: GuestVa, value: T) -> Result<()> {
        self.write_bytes(va, value.to_le().as_ref())
    }

    /// Fills `buf` from guest memory at `va`; every byte must be mapped and readable.
    pub fn read_bytes(&self, va: GuestVa, buf: &mut [u8]) -> Result<()> {
        let guest = self.checked_bytes(va, buf.len(), Access::Read)?;
        for (offset, out) in buf.iter_mut().enumerate() {
            // SAFETY: `checked_bytes` proved the range is mapped, hence committed and inside
            // the reservation, and this view keeps it mapped. Another thread may access the
            // same guest byte, so the access is atomic.
            *out = unsafe { AtomicU8::from_ptr(guest.add(offset)) }.load(Ordering::Relaxed);
        }
        Ok(())
    }

    /// Copies `data` to guest memory at `va`; every byte must be mapped and writable, and none
    /// is written otherwise.
    pub fn write_bytes(&self, va: GuestVa, data: &[u8]) -> Result<()> {
        let guest = self.checked_bytes(va, data.len(), Access::Write)?;
        for (offset, byte) in data.iter().enumerate() {
            // SAFETY: as in `read_bytes`; mapped guest bytes are committed read-write.
            unsafe { AtomicU8::from_ptr(guest.add(offset)) }.store(*byte, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Checks that `[va, va + len)` is mapped with `access` allowed; returns its host address.
    fn checked_bytes(&self, va: GuestVa, len: usize, access: Access) -> Result<*mut u8> {
        let end = va
            .get()
            .checked_add(len as u64)
            .filter(|end| *end <= self.space.size)
            .ok_or(MemError::Unmapped { va, access })?;
        let mut addr = va.get();
        while addr < end {
            let Some((_, region)) = self.regions.region_at(addr) else {
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
        // SAFETY: `va + len` is at most the space size, the length of the reservation, so the
        // result stays inside it.
        Ok(unsafe { self.host_base().as_ptr().add(to_usize(va.get())) })
    }
}

impl Regions {
    /// Makes `addr` a region boundary: a region that contains it is split in two.
    fn split_at(&mut self, addr: u64) {
        if let Some((start, region)) = self.region_at(addr)
            && start < addr
        {
            self.0.insert(
                start,
                Region {
                    end: addr,
                    ..region
                },
            );
            self.0.insert(addr, region);
        }
    }

    /// Merges adjacent regions with the same permission from the region that ends at `lo` up
    /// to the region that starts at `hi`.
    fn merge(&mut self, lo: u64, hi: u64) {
        let mut cursor = self
            .0
            .range(..lo)
            .next_back()
            .filter(|(_, region)| region.end == lo)
            .map_or(lo, |(&start, _)| start);
        while let Some(&region) = self.0.get(&cursor) {
            if region.end > hi {
                break;
            }
            match self.0.get(&region.end).copied() {
                Some(next) if next.perm == region.perm => {
                    self.0.remove(&region.end);
                    self.0.insert(
                        cursor,
                        Region {
                            end: next.end,
                            ..region
                        },
                    );
                }
                _ => cursor = region.end,
            }
        }
    }

    fn region_at(&self, addr: u64) -> Option<(u64, Region)> {
        let (&start, &region) = self.0.range(..=addr).next_back()?;
        (addr < region.end).then_some((start, region))
    }

    fn overlaps(&self, start: u64, end: u64) -> bool {
        self.0
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
