// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Unix host memory through `rustix`: `mmap(PROT_NONE)` reserves, `mprotect` commits.

use std::io;
use std::ptr::{self, NonNull};

use rustix::mm::{
    Advice, MapFlags, MprotectFlags, ProtFlags, madvise, mmap_anonymous, mprotect, munmap,
};

/// Linux frees the pages at once; elsewhere the POSIX advice is only a hint.
#[cfg(any(target_os = "linux", target_os = "android"))]
const DROP_PAGES: Advice = Advice::LinuxDontNeed;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const DROP_PAGES: Advice = Advice::DontNeed;

pub(super) fn page_size() -> usize {
    rustix::param::page_size()
}

pub(super) fn reserve(len: usize) -> io::Result<NonNull<u8>> {
    // SAFETY: a null address lets the kernel pick a free range, so no existing mapping is
    // replaced; the mapping is inaccessible until committed.
    let ptr = unsafe {
        mmap_anonymous(
            ptr::null_mut(),
            len,
            ProtFlags::empty(),
            MapFlags::PRIVATE | MapFlags::NORESERVE,
        )
    }?;
    NonNull::new(ptr.cast()).ok_or_else(|| io::Error::other("mmap returned a null address"))
}

/// # Safety
/// `[start, start + len)` must lie inside a live reservation from [`reserve`].
pub(super) unsafe fn commit(start: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees the range belongs to our reservation.
    unsafe {
        mprotect(
            start.as_ptr().cast(),
            len,
            MprotectFlags::READ | MprotectFlags::WRITE,
        )
    }?;
    Ok(())
}

/// # Safety
/// `[start, start + len)` must lie inside a live reservation from [`reserve`], with no live
/// references into it.
pub(super) unsafe fn decommit(start: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees the range belongs to our reservation.
    unsafe { mprotect(start.as_ptr().cast(), len, MprotectFlags::empty()) }?;
    // SAFETY: as above; dropping the pages of a private anonymous mapping only discards data.
    unsafe { madvise(start.as_ptr().cast(), len, DROP_PAGES) }?;
    Ok(())
}

/// # Safety
/// `start`/`len` must be exactly a live reservation from [`reserve`], with no live references
/// into it.
pub(super) unsafe fn release(start: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees this is a whole reservation of ours.
    unsafe { munmap(start.as_ptr().cast(), len) }?;
    Ok(())
}
