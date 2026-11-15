// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `WaitSynchronization` and `CancelSynchronization` (docs/research/b2-svcs-and-ipc.md V10,
//! V11, Switch 1 knowledge).
//!
//! A thread waits on readable events and threads. The wait ends with success and the index
//! of a signaled object, with `TIMED_OUT` when its timeout passes (the kernel timer fires for
//! it), or with `CANCELLED`. A guest core blocks only here, on guest synchronization (ADR-0004):
//! the waiting thread leaves its core, which runs other threads or parks.

use std::time::{Duration, Instant};

use lince_common::GuestVa;
use lince_mem::AddressSpace;

use crate::event_svc::INVALID_HANDLE;
use crate::handle::Object;
use crate::kernel::{Kernel, Outcome, Regs};
use crate::svc::{ResultCode, SUCCESS, done};
use crate::thread::{CURRENT_THREAD, MAX_WAIT_OBJECTS, Status, WaitSet, Waitable};

/// The handle array cannot be read (V10).
const INVALID_ADDRESS: ResultCode = 0xE601;
/// The timeout passed before any object was signaled (V10).
pub(crate) const TIMED_OUT: ResultCode = 0xEA01;
/// `CancelSynchronization` ended the wait (V10).
const CANCELLED: ResultCode = 0xEC01;
/// More handles than one wait takes (V10).
const TOO_MANY_HANDLES: ResultCode = 0xEE01;

/// The low 32 bits of a register: its `W` view.
fn w(reg: u64) -> u32 {
    u32::try_from(reg & 0xFFFF_FFFF).unwrap_or(u32::MAX)
}

/// What a waiter can wait on through `handle`.
// GUESS: the note does not say whether the current-thread pseudo-handle is waitable; a thread
// waiting for itself would never wake, so it is rejected like a bad handle.
fn waitable(kernel: &Kernel, handle: u32) -> Option<Waitable> {
    if handle == CURRENT_THREAD {
        return None;
    }
    match kernel.handles.get(handle)? {
        Object::ReadableEvent(event) => Some(Waitable::Event(event)),
        Object::Thread(index) => Some(Waitable::Thread(index)),
        Object::WritableEvent(_) => None,
    }
}

fn signaled(kernel: &Kernel, object: Waitable) -> bool {
    match object {
        Waitable::Event(event) => kernel.events[event],
        Waitable::Thread(index) => kernel.threads.status(index) == Status::Exited,
    }
}

/// `X1` handle array, `W2` count, `X3` timeout (signed nanoseconds; negative waits forever, 0
/// only polls); out `W0`, `W1` index of the signaled object (V10).
pub(crate) fn wait_synchronization(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let Some(current) = kernel.current else {
        return Outcome::Unsupported;
    };
    let count = usize::try_from(w(regs[2])).unwrap_or(usize::MAX);
    if count > MAX_WAIT_OBJECTS {
        return done(regs, TOO_MANY_HANDLES);
    }
    let mut set = WaitSet::default();
    for n in 0..count {
        let Ok(handle) = mem.read::<u32>(GuestVa::new(regs[1]).wrapping_add(4 * n as u64)) else {
            return done(regs, INVALID_ADDRESS);
        };
        let Some(object) = waitable(kernel, handle) else {
            return done(regs, INVALID_HANDLE);
        };
        // Cannot fail: `count` is at most the set's capacity.
        set.push(object);
    }
    // GUESS: a pending cancellation is used only by a wait whose handles are all valid, as on
    // Switch 1; V10 does not give the order of the checks.
    if kernel.threads.take_cancel_pending(current.index) {
        return done(regs, CANCELLED);
    }
    if let Some(which) = set.objects().iter().position(|&o| signaled(kernel, o)) {
        regs[1] = which as u64;
        return done(regs, SUCCESS);
    }
    let timeout = regs[3].cast_signed();
    if timeout == 0 {
        return done(regs, TIMED_OUT);
    }
    let until = match u64::try_from(timeout) {
        // UNKNOWN: a timeout past the host clock's range waits forever.
        Ok(ns) => Instant::now().checked_add(Duration::from_nanos(ns)),
        Err(_) => None,
    };
    kernel.threads.begin_wait(current.index, &set, until);
    Outcome::Reschedule
}

/// `W0` thread handle; out `W0` (V11). A waiting thread's wait ends with `CANCELLED`; a thread
/// that is not waiting has its next wait cancelled instead.
// GUESS: the pending cancellation follows the Switch 1 behavior; the note lists only the
// invalid-handle result.
pub(crate) fn cancel_synchronization(
    kernel: &mut Kernel,
    _: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let handle = w(regs[0]);
    let index = match (handle, kernel.current) {
        (CURRENT_THREAD, Some(current)) => Some(current.index),
        _ => match kernel.handles.get(handle) {
            Some(Object::Thread(index)) => Some(index),
            Some(Object::ReadableEvent(_) | Object::WritableEvent(_)) | None => None,
        },
    };
    let Some(index) = index else {
        return done(regs, INVALID_HANDLE);
    };
    if !kernel.threads.end_wait(index, CANCELLED, None) {
        kernel.threads.set_cancel_pending(index);
    }
    done(regs, SUCCESS)
}
