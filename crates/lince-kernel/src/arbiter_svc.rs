// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The mutex and the condition variable by address arbitration (ADR-0024 B2 scope;
//! docs/research/b2-svcs-and-ipc.md V12, V13, Switch 1 knowledge).
//!
//! Both live in guest memory; the kernel only queues the threads that must block. The mutex
//! word holds the owner's tag (its thread handle) with `WAITERS` set while threads wait for
//! it: guest code takes and releases a free mutex with its own atomics and calls the kernel
//! only when the word says there is contention. The key word of a condition variable is 1
//! while threads may wait on it.
//!
//! GUESS (the note lists no semantics or results; tests/homebrew/b2.s reads them the same
//! way): `ArbitrateLock` returns at once if the word no longer names `owner | WAITERS`, and
//! otherwise returns owning the mutex, handed over by `ArbitrateUnlock`, which picks the
//! highest-priority waiter. A signaled condition-variable waiter reacquires the mutex before
//! it returns; one that times out returns without it. There is no priority inheritance. The
//! kernel does not check that the caller of `ArbitrateUnlock` (or of a condition-variable
//! wait) owns the mutex. `SignalProcessWideKey` with a count of 0 or less wakes every waiter.
//! A mutex word unmapped while threads wait for it leaves the woken owner unaware of the
//! failed handoff, which the unlocker sees as an invalid address.
//!
//! Every guest-word access is a host compare-and-swap (`AddressSpace::compare_exchange_u32`),
//! so cores running guest atomics on the same word never see it torn.

use std::time::{Duration, Instant};

use lince_common::GuestVa;
use lince_mem::AddressSpace;

use crate::event_svc::INVALID_HANDLE;
use crate::handle::Object;
use crate::kernel::{Kernel, Outcome, Regs};
use crate::svc::{SUCCESS, done};
use crate::thread::WaitKind;
use crate::wait_svc::{INVALID_ADDRESS, TIMED_OUT};

/// The mutex-word bit that says threads wait for the mutex (tests/homebrew/b2.s, GUESS).
const WAITERS: u32 = 0x4000_0000;

/// The low 32 bits of a register: its `W` view.
fn w(reg: u64) -> u32 {
    u32::try_from(reg & 0xFFFF_FFFF).unwrap_or(u32::MAX)
}

/// The guest word at `va`, read atomically; `None` if it is not an aligned, mapped, writable
/// word.
fn load(mem: &AddressSpace, va: GuestVa) -> Option<u32> {
    match mem.compare_exchange_u32(va, 0, 0).ok()? {
        Ok(zero) => Some(zero),
        Err(value) => Some(value),
    }
}

/// Sets the guest word at `va` to `value`; `None` if it is not an aligned, mapped, writable
/// word. Retries while guest atomics change it meanwhile.
fn store(mem: &AddressSpace, va: GuestVa, value: u32) -> Option<()> {
    let mut seen = load(mem, va)?;
    while let Err(now) = mem.compare_exchange_u32(va, seen, value).ok()? {
        seen = now;
    }
    Some(())
}

/// Hands the mutex at `address` to its best waiter, or frees it; `None` if the word cannot be
/// accessed.
fn release(kernel: &mut Kernel, mem: &AddressSpace, address: GuestVa) -> Option<()> {
    let waits_here =
        |kind: &WaitKind| matches!(kind, WaitKind::Mutex { address: a, .. } if *a == address);
    let Some(next) = kernel.threads.best_waiter(waits_here) else {
        return store(mem, address, 0);
    };
    let WaitKind::Mutex { tag, .. } = *kernel.threads.thread(next).wait() else {
        return None;
    };
    // The new owner is no longer a waiter; others may remain.
    kernel.threads.end_wait(next, SUCCESS, None);
    let more = kernel.threads.any_waiter(waits_here);
    store(mem, address, if more { tag | WAITERS } else { tag })
}

/// Makes waiting thread `index` take the mutex at `mutex` as `tag`: at once if it is free,
/// else as a mutex waiter, with `WAITERS` set; `None` if the word cannot be accessed.
fn reacquire(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    index: usize,
    mutex: GuestVa,
    tag: u32,
) -> Option<()> {
    let mut seen = 0;
    loop {
        let wanted = if seen == 0 { tag } else { seen | WAITERS };
        match mem.compare_exchange_u32(mutex, seen, wanted).ok()? {
            Ok(_) if seen == 0 => {
                kernel.threads.end_wait(index, SUCCESS, None);
                return Some(());
            }
            Ok(_) => {
                let address = mutex;
                kernel
                    .threads
                    .rewait(index, &WaitKind::Mutex { address, tag });
                return Some(());
            }
            Err(now) => seen = now,
        }
    }
}

/// `W0` owner handle, `X1` mutex address, `W2` tag (the caller's handle); out `W0` (V12).
pub(crate) fn arbitrate_lock(kernel: &mut Kernel, mem: &AddressSpace, regs: &mut Regs) -> Outcome {
    let Some(current) = kernel.current else {
        return Outcome::Unsupported;
    };
    let (owner, address, tag) = (w(regs[0]), GuestVa::new(regs[1]), w(regs[2]));
    let Some(word) = load(mem, address) else {
        return done(regs, INVALID_ADDRESS);
    };
    if word != owner | WAITERS {
        // The mutex changed since the caller looked; it retries in guest code.
        return done(regs, SUCCESS);
    }
    if !matches!(kernel.handles.get(owner), Some(Object::Thread(_))) {
        return done(regs, INVALID_HANDLE);
    }
    kernel
        .threads
        .begin_wait(current.index, &WaitKind::Mutex { address, tag }, None);
    Outcome::Reschedule
}

/// `X0` mutex address; out `W0` (V12). The caller owns the mutex.
pub(crate) fn arbitrate_unlock(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    if kernel.current.is_none() {
        return Outcome::Unsupported;
    }
    let code = match release(kernel, mem, GuestVa::new(regs[0])) {
        Some(()) => SUCCESS,
        None => INVALID_ADDRESS,
    };
    done(regs, code)
}

/// `X0` key address, `X1` mutex address, `W2` tag, `X3` timeout (signed nanoseconds; negative
/// waits forever); out `W0` (V13). Releases the mutex, then waits on the key.
pub(crate) fn wait_process_wide_key_atomic(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    let Some(current) = kernel.current else {
        return Outcome::Unsupported;
    };
    let (key, mutex, tag) = (GuestVa::new(regs[0]), GuestVa::new(regs[1]), w(regs[2]));
    if load(mem, key).is_none() || release(kernel, mem, mutex).is_none() {
        return done(regs, INVALID_ADDRESS);
    }
    if store(mem, key, 1).is_none() {
        return done(regs, INVALID_ADDRESS);
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
    let kind = WaitKind::Key { key, mutex, tag };
    kernel.threads.begin_wait(current.index, &kind, until);
    Outcome::Reschedule
}

/// `X0` key address, `W1` count (signed; 0 or less wakes every waiter); no result (V13).
/// Wakes waiters in priority order; each reacquires its mutex.
pub(crate) fn signal_process_wide_key(
    kernel: &mut Kernel,
    mem: &AddressSpace,
    regs: &mut Regs,
) -> Outcome {
    if kernel.current.is_none() {
        return Outcome::Unsupported;
    }
    let key = GuestVa::new(regs[0]);
    let count = w(regs[1]).cast_signed();
    let on_key = |kind: &WaitKind| matches!(kind, WaitKind::Key { key: k, .. } if *k == key);
    let mut woken = 0;
    while count <= 0 || woken < count {
        let Some(index) = kernel.threads.best_waiter(on_key) else {
            break;
        };
        let WaitKind::Key { mutex, tag, .. } = *kernel.threads.thread(index).wait() else {
            break;
        };
        // GUESS: a mutex word the kernel cannot access ends the wait with that error.
        if reacquire(kernel, mem, index, mutex, tag).is_none() {
            kernel.threads.end_wait(index, INVALID_ADDRESS, None);
        }
        woken += 1;
    }
    if !kernel.threads.any_waiter(on_key) {
        // GUESS: an inaccessible key was already rejected when the waiters started.
        let _ = store(mem, key, 0);
    }
    Outcome::Resume
}
