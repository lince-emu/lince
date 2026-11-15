// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Events and `CloseHandle` (docs/research/b2-svcs-and-ipc.md V7–V9, Switch 1 knowledge).
//!
//! An event has a writable end, which signals it, and a readable end, which programs wait on
//! and reset. It stays signaled until cleared or reset; signaling it ends every wait on it.

use lince_mem::AddressSpace;

use crate::handle::Object;
use crate::kernel::{Kernel, Outcome, Regs};
use crate::svc::{OUT_OF_RESOURCE, ResultCode, SUCCESS, done};
use crate::thread::Waitable;

/// The handle names no open object of the kind the SVC takes (V7).
pub(crate) const INVALID_HANDLE: ResultCode = 0xE401;
/// `ResetSignal` on an event that is not signaled (V7).
const NOT_SIGNALED: ResultCode = 0xFA01;

/// Most events a process may create over its life.
// GUESS: Horizon's limit comes from the process's resource limit. Known B2 limitation: closed
// events keep their entry, so the limit counts creations, not live events.
const MAX_EVENTS: usize = 256;

/// The low 32 bits of a register: its `W` view.
fn w(reg: u64) -> u32 {
    u32::try_from(reg & 0xFFFF_FFFF).unwrap_or(u32::MAX)
}

/// No arguments; out `W0`, `W1` writable handle, `W2` readable handle (V8).
pub(crate) fn create_event(kernel: &mut Kernel, _: &AddressSpace, regs: &mut Regs) -> Outcome {
    let event = kernel.events.len();
    if event >= MAX_EVENTS {
        return done(regs, OUT_OF_RESOURCE);
    }
    let Some(writable) = kernel.handles.open(Object::WritableEvent(event)) else {
        return done(regs, OUT_OF_RESOURCE);
    };
    let Some(readable) = kernel.handles.open(Object::ReadableEvent(event)) else {
        kernel.handles.close(writable);
        return done(regs, OUT_OF_RESOURCE);
    };
    kernel.events.push(false);
    regs[1] = u64::from(writable);
    regs[2] = u64::from(readable);
    done(regs, SUCCESS)
}

/// `W0` writable handle; out `W0` (V7).
pub(crate) fn signal_event(kernel: &mut Kernel, _: &AddressSpace, regs: &mut Regs) -> Outcome {
    match kernel.handles.get(w(regs[0])) {
        Some(Object::WritableEvent(event)) => {
            kernel.events[event] = true;
            kernel.threads.wake_waiters(Waitable::Event(event));
            done(regs, SUCCESS)
        }
        Some(Object::ReadableEvent(_) | Object::Thread(_) | Object::Session(_)) | None => {
            done(regs, INVALID_HANDLE)
        }
    }
}

/// `W0` handle of either end; out `W0` (V7). Clearing an event that is not signaled succeeds.
// GUESS: the note does not say which end ClearEvent takes; it accepts both.
pub(crate) fn clear_event(kernel: &mut Kernel, _: &AddressSpace, regs: &mut Regs) -> Outcome {
    match kernel.handles.get(w(regs[0])) {
        Some(Object::WritableEvent(event) | Object::ReadableEvent(event)) => {
            kernel.events[event] = false;
            done(regs, SUCCESS)
        }
        Some(Object::Thread(_) | Object::Session(_)) | None => done(regs, INVALID_HANDLE),
    }
}

/// `W0` readable handle; out `W0`: clears a signaled event, else `NOT_SIGNALED` (V7).
pub(crate) fn reset_signal(kernel: &mut Kernel, _: &AddressSpace, regs: &mut Regs) -> Outcome {
    match kernel.handles.get(w(regs[0])) {
        Some(Object::ReadableEvent(event)) => {
            let signaled = std::mem::take(&mut kernel.events[event]);
            done(regs, if signaled { SUCCESS } else { NOT_SIGNALED })
        }
        Some(Object::WritableEvent(_) | Object::Thread(_) | Object::Session(_)) | None => {
            done(regs, INVALID_HANDLE)
        }
    }
}

/// `W0` handle; out `W0` (V9). Closing a thread's handle does not end the thread; closing a
/// session's handle drops its object.
// GUESS: the note lists no result for a handle that is not open, a double close included, nor
// for pseudo-handles such as the current thread (O4); all return the invalid-handle code the
// other SVCs return.
pub(crate) fn close_handle(kernel: &mut Kernel, _: &AddressSpace, regs: &mut Regs) -> Outcome {
    let code = match kernel.handles.close(w(regs[0])) {
        Some(Object::Session(session)) => {
            kernel.sessions[session] = None;
            SUCCESS
        }
        Some(Object::Thread(_) | Object::ReadableEvent(_) | Object::WritableEvent(_)) => SUCCESS,
        None => INVALID_HANDLE,
    };
    done(regs, code)
}
