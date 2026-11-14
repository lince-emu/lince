// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Windows host memory through `windows-sys`: `MEM_RESERVE` reserves, `MEM_COMMIT` commits.

use std::io;
use std::mem::MaybeUninit;
use std::ptr::{self, NonNull};

use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_NOACCESS, PAGE_READWRITE,
    VirtualAlloc, VirtualFree,
};
use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};

pub(super) fn page_size() -> usize {
    let mut info = MaybeUninit::<SYSTEM_INFO>::uninit();
    // SAFETY: `GetSystemInfo` fills the whole struct and cannot fail.
    let info = unsafe {
        GetSystemInfo(info.as_mut_ptr());
        info.assume_init()
    };
    info.dwPageSize as usize
}

pub(super) fn reserve(len: usize) -> io::Result<NonNull<u8>> {
    // SAFETY: a null address lets the OS pick a free range; nothing existing is touched.
    let ptr = unsafe { VirtualAlloc(ptr::null(), len, MEM_RESERVE, PAGE_NOACCESS) };
    NonNull::new(ptr.cast()).ok_or_else(io::Error::last_os_error)
}

/// # Safety
/// `[start, start + len)` must lie inside a live reservation from [`reserve`].
pub(super) unsafe fn commit(start: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees the range belongs to our reservation.
    let ptr = unsafe { VirtualAlloc(start.as_ptr().cast(), len, MEM_COMMIT, PAGE_READWRITE) };
    if ptr.is_null() {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// # Safety
/// `[start, start + len)` must lie inside a live reservation from [`reserve`], with no live
/// references into it.
pub(super) unsafe fn decommit(start: NonNull<u8>, len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees the range belongs to our reservation.
    if unsafe { VirtualFree(start.as_ptr().cast(), len, MEM_DECOMMIT) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// # Safety
/// `start` must be a live reservation from [`reserve`], with no live references into it.
pub(super) unsafe fn release(start: NonNull<u8>, _len: usize) -> io::Result<()> {
    // SAFETY: the caller guarantees this is a whole reservation of ours; `MEM_RELEASE` takes
    // a size of 0.
    if unsafe { VirtualFree(start.as_ptr().cast(), 0, MEM_RELEASE) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
