// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The handle table, events and `CloseHandle` (T-052 step 1), through the SVC dispatcher.

use lince_common::GuestVa;
use lince_kernel::{Flow, Kernel, Regs, SvcNumber, ThreadParams};
use lince_mem::{AddressSpace, SpaceConfig};
use lince_trace::Trace;

const SIGNAL_EVENT: SvcNumber = 0x11;
const CLEAR_EVENT: SvcNumber = 0x12;
const CLOSE_HANDLE: SvcNumber = 0x16;
const RESET_SIGNAL: SvcNumber = 0x17;
const CREATE_EVENT: SvcNumber = 0x45;

const SUCCESS: u64 = 0;
const INVALID_HANDLE: u64 = 0xE401;
const NOT_SIGNALED: u64 = 0xFA01;

struct Fixture {
    kernel: Kernel,
    mem: AddressSpace,
}

impl Fixture {
    fn new() -> Self {
        Self {
            kernel: Kernel::new(&mut Trace::new(16)).expect("test setup"),
            mem: AddressSpace::new(SpaceConfig::for_host()).expect("test setup"),
        }
    }

    /// Runs SVC `number` with `W0` = `handle` and returns the registers after it.
    fn svc(&mut self, number: SvcNumber, handle: u64) -> Regs {
        let mut regs = Regs::default();
        regs[0] = handle;
        let flow = self
            .kernel
            .dispatch(&self.mem, number, &mut regs)
            .expect("the SVC is implemented");
        assert_eq!(flow, Flow::Continue);
        regs
    }

    /// Creates an event and returns its writable and readable handles.
    fn event(&mut self) -> (u64, u64) {
        let regs = self.svc(CREATE_EVENT, 0);
        assert_eq!(regs[0], SUCCESS);
        (regs[1], regs[2])
    }
}

#[test]
fn an_event_stays_signaled_until_reset_once() {
    let mut f = Fixture::new();
    let (writable, readable) = f.event();
    assert_ne!(writable, readable);

    assert_eq!(
        f.svc(RESET_SIGNAL, readable)[0],
        NOT_SIGNALED,
        "starts clear"
    );
    assert_eq!(f.svc(SIGNAL_EVENT, writable)[0], SUCCESS);
    assert_eq!(
        f.svc(SIGNAL_EVENT, writable)[0],
        SUCCESS,
        "signaling twice is fine"
    );
    assert_eq!(f.svc(RESET_SIGNAL, readable)[0], SUCCESS);
    assert_eq!(
        f.svc(RESET_SIGNAL, readable)[0],
        NOT_SIGNALED,
        "reset cleared it"
    );
}

#[test]
fn clear_event_clears_from_either_end() {
    let mut f = Fixture::new();
    let (writable, readable) = f.event();
    for end in [writable, readable] {
        f.svc(SIGNAL_EVENT, writable);
        assert_eq!(f.svc(CLEAR_EVENT, end)[0], SUCCESS);
        assert_eq!(f.svc(RESET_SIGNAL, readable)[0], NOT_SIGNALED);
    }
}

#[test]
fn each_end_takes_only_the_svcs_meant_for_it() {
    let mut f = Fixture::new();
    let (writable, readable) = f.event();
    assert_eq!(f.svc(SIGNAL_EVENT, readable)[0], INVALID_HANDLE);
    assert_eq!(f.svc(RESET_SIGNAL, writable)[0], INVALID_HANDLE);
}

#[test]
fn closing_a_handle_twice_returns_invalid_handle() {
    let mut f = Fixture::new();
    let (writable, readable) = f.event();
    assert_eq!(f.svc(CLOSE_HANDLE, writable)[0], SUCCESS);
    assert_eq!(f.svc(CLOSE_HANDLE, writable)[0], INVALID_HANDLE);
    assert_eq!(
        f.svc(SIGNAL_EVENT, writable)[0],
        INVALID_HANDLE,
        "closed for good"
    );
    assert_eq!(
        f.svc(RESET_SIGNAL, readable)[0],
        NOT_SIGNALED,
        "the other end lives on"
    );
}

#[test]
fn a_new_handle_in_a_closed_slot_is_not_the_old_one() {
    let mut f = Fixture::new();
    let (writable, readable) = f.event();
    f.svc(CLOSE_HANDLE, writable);
    f.svc(CLOSE_HANDLE, readable);
    let (new_writable, _) = f.event();
    assert_ne!(new_writable, writable);
    assert_eq!(f.svc(SIGNAL_EVENT, writable)[0], INVALID_HANDLE);
    assert_eq!(f.svc(SIGNAL_EVENT, new_writable)[0], SUCCESS);
}

#[test]
fn a_thread_handle_is_closed_like_any_other_but_is_no_event() {
    let mut f = Fixture::new();
    let params = ThreadParams {
        entry: GuestVa::new(0x1_0000),
        arg: 0,
        stack_top: 0,
        priority: 44,
        core: 0,
    };
    let thread = u64::from(f.kernel.spawn_thread(&f.mem, params).expect("test setup"));
    assert_eq!(f.svc(SIGNAL_EVENT, thread)[0], INVALID_HANDLE);
    assert_eq!(f.svc(CLOSE_HANDLE, thread)[0], SUCCESS);
    assert_eq!(f.svc(CLOSE_HANDLE, thread)[0], INVALID_HANDLE);
}

#[test]
fn handle_zero_and_unknown_values_are_invalid() {
    let mut f = Fixture::new();
    for handle in [0, 0x1234_5678, u64::MAX] {
        assert_eq!(
            f.svc(CLOSE_HANDLE, handle)[0],
            INVALID_HANDLE,
            "{handle:#x}"
        );
    }
}
