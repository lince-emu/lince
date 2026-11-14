// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Host virtual memory (PAT-08): one safe API; `unsafe` OS calls stay inside `imp`.

use std::io;
use std::ptr::NonNull;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as imp;

/// The host OS page size in bytes.
pub(crate) fn page_size() -> usize {
    imp::page_size()
}

/// A reserved, initially inaccessible range of host address space, released on drop.
pub(crate) struct Reservation {
    base: NonNull<u8>,
    len: usize,
}

impl Reservation {
    /// Reserves `len` bytes without committing memory. `len` must be a multiple of the host
    /// page size.
    pub(crate) fn new(len: usize) -> io::Result<Self> {
        let base = imp::reserve(len)?;
        Ok(Self { base, len })
    }

    /// The first byte of the reservation.
    pub(crate) fn base(&self) -> NonNull<u8> {
        self.base
    }

    /// Makes `[offset, offset + len)` readable, writable and backed by memory. The range must
    /// be host-page aligned.
    pub(crate) fn commit(&mut self, offset: usize, len: usize) -> io::Result<()> {
        let start = self.range(offset, len)?;
        // SAFETY: `range` checked that the range lies inside this live reservation.
        unsafe { imp::commit(start, len) }
    }

    /// Makes `[offset, offset + len)` inaccessible again and lets the host reclaim it. The
    /// range must be host-page aligned.
    pub(crate) fn decommit(&mut self, offset: usize, len: usize) -> io::Result<()> {
        let start = self.range(offset, len)?;
        // SAFETY: `range` checked that the range lies inside this live reservation.
        unsafe { imp::decommit(start, len) }
    }

    fn range(&self, offset: usize, len: usize) -> io::Result<NonNull<u8>> {
        match offset.checked_add(len) {
            Some(end) if end <= self.len => {
                // SAFETY: `offset <= self.len`, so the result stays inside the reservation.
                Ok(unsafe { self.base.add(offset) })
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "range outside the reservation",
            )),
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // SAFETY: `base`/`len` describe a reservation made by `imp::reserve` and released only
        // here; no reference into it outlives `self`.
        let released = unsafe { imp::release(self.base, self.len) };
        debug_assert!(
            released.is_ok(),
            "releasing a reservation failed: {released:?}"
        );
    }
}
