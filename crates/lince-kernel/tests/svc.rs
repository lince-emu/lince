// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Each B1 SVC through the dispatch API, and the unknown-SVC contract (QUAL-73).
//!
//! Numbers, registers and result codes follow docs/research/nro-and-b1-svcs.md S2–S11.

use lince_common::GuestVa;
use lince_kernel::{Flow, HEAP_BASE, Kernel, KernelError, Regs, svc_name};
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission, SpaceConfig};
use lince_trace::{EventKind, Trace};

const PAGE: u64 = GUEST_PAGE_SIZE;
const MIB2: u64 = 0x20_0000;
const RW: Permission = Permission::READ.union(Permission::WRITE);

struct Fixture {
    kernel: Kernel,
    mem: AddressSpace,
    trace: Trace,
}

fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let mut trace = Trace::new(16);
    let kernel = Kernel::new(&mut trace)?;
    let mem = AddressSpace::new(SpaceConfig::for_host())?;
    Ok(Fixture { kernel, mem, trace })
}

fn va(raw: u64) -> GuestVa {
    GuestVa::new(raw)
}

impl Fixture {
    /// Dispatches `number` with `args` in `X0..`; returns the flow and the registers after.
    fn svc(&mut self, number: u8, args: &[u64]) -> lince_kernel::Result<(Flow, Regs)> {
        let mut regs: Regs = [0; 8];
        regs[..args.len()].copy_from_slice(args);
        let flow = self.kernel.dispatch(&mut self.mem, number, &mut regs)?;
        Ok((flow, regs))
    }

    /// Dispatches an SVC that must resume the guest; returns its result code (`W0`).
    fn result(&mut self, number: u8, args: &[u64]) -> lince_kernel::Result<u64> {
        let (flow, regs) = self.svc(number, args)?;
        assert_eq!(flow, Flow::Continue);
        Ok(regs[0])
    }
}

#[test]
fn set_heap_size_maps_a_zeroed_read_write_heap() {
    let mut f = fixture().unwrap();
    let (_, regs) = f.svc(0x01, &[0, 2 * MIB2]).unwrap();

    assert_eq!((regs[0], regs[1]), (0, HEAP_BASE.get()));
    assert_eq!(f.mem.permission(HEAP_BASE), Some(RW));
    f.mem
        .write(va(HEAP_BASE.get() + 2 * MIB2 - 8), 7u64)
        .unwrap();
    assert_eq!(f.mem.read::<u64>(va(HEAP_BASE.get())).unwrap(), 0);
}

#[test]
fn growing_the_heap_keeps_its_contents_and_shrinking_to_zero_unmaps_it() {
    let mut f = fixture().unwrap();
    f.svc(0x01, &[0, MIB2]).unwrap();
    f.mem.write(HEAP_BASE, 0xABCD_u64).unwrap();

    assert_eq!(f.result(0x01, &[0, 3 * MIB2]).unwrap(), 0);
    assert_eq!(f.mem.read::<u64>(HEAP_BASE).unwrap(), 0xABCD);
    assert!(f.mem.permission(va(HEAP_BASE.get() + 2 * MIB2)).is_some());

    assert_eq!(f.result(0x01, &[0, MIB2]).unwrap(), 0);
    assert!(f.mem.permission(va(HEAP_BASE.get() + MIB2)).is_none());
    assert_eq!(f.mem.read::<u64>(HEAP_BASE).unwrap(), 0xABCD);

    assert_eq!(f.result(0x01, &[0, 0]).unwrap(), 0);
    assert!(f.mem.permission(HEAP_BASE).is_none());
}

#[test]
fn set_heap_size_rejects_unaligned_and_oversized_requests() {
    let mut f = fixture().unwrap();
    assert_eq!(f.result(0x01, &[0, MIB2 + PAGE]).unwrap(), 0xCA01);
    assert_eq!(f.result(0x01, &[0, 0x1_0000_0000 + MIB2]).unwrap(), 0xCA01);
    assert!(f.mem.permission(HEAP_BASE).is_none());
}

#[test]
fn set_memory_permission_changes_a_region_and_checks_its_arguments() {
    let mut f = fixture().unwrap();
    f.mem.map(va(0x10_0000), 2 * PAGE, RW).unwrap();

    assert_eq!(f.result(0x02, &[0x10_0000, 2 * PAGE, 1]).unwrap(), 0);
    assert_eq!(f.mem.permission(va(0x10_0000)), Some(Permission::READ));
    assert_eq!(f.result(0x02, &[0x10_0000, 2 * PAGE, 3]).unwrap(), 0);
    assert_eq!(f.mem.permission(va(0x10_0000)), Some(RW));

    assert_eq!(f.result(0x02, &[0x10_0010, 2 * PAGE, 1]).unwrap(), 0xCC01);
    assert_eq!(f.result(0x02, &[0x10_0000, 0, 1]).unwrap(), 0xCA01);
    assert_eq!(f.result(0x02, &[0x10_0000, PAGE + 1, 1]).unwrap(), 0xCA01);
    assert_eq!(f.result(0x02, &[0x10_0000, 2 * PAGE, 4]).unwrap(), 0xD801);
    assert_eq!(f.result(0x02, &[0x10_0000, 2 * PAGE, 2]).unwrap(), 0xD801);
    assert_eq!(f.result(0x02, &[0x20_0000, PAGE, 1]).unwrap(), 0xD401);
    assert_eq!(f.mem.permission(va(0x10_0000)), Some(RW));
}

#[test]
fn set_memory_attribute_accepts_mapped_memory_only() {
    let mut f = fixture().unwrap();
    f.mem.map(va(0x10_0000), 2 * PAGE, RW).unwrap();

    assert_eq!(f.result(0x03, &[0x10_0000, PAGE, 8, 8]).unwrap(), 0);
    assert_eq!(f.result(0x03, &[0x10_0000, 2 * PAGE, 8, 0]).unwrap(), 0);
    assert_eq!(
        f.result(0x03, &[0x10_0000, 3 * PAGE, 8, 8]).unwrap(),
        0xD401
    );
    assert_eq!(f.result(0x03, &[0x20_0000, PAGE, 8, 8]).unwrap(), 0xD401);
    assert_eq!(f.result(0x03, &[0x10_0001, PAGE, 8, 8]).unwrap(), 0xCC01);
    assert_eq!(f.result(0x03, &[0x10_0000, 0, 8, 8]).unwrap(), 0xCA01);
}

#[test]
fn set_memory_attribute_refuses_bits_other_than_uncached() {
    let mut f = fixture().unwrap();
    f.mem.map(va(0x10_0000), PAGE, RW).unwrap();
    let unimplemented = KernelError::Unimplemented {
        number: 0x03,
        name: "SetMemoryAttribute",
    };

    assert_eq!(
        f.svc(0x03, &[0x10_0000, PAGE, 0x10, 0x10]).unwrap_err(),
        unimplemented
    );
    assert_eq!(
        f.svc(0x03, &[0x10_0000, PAGE, 8, 1]).unwrap_err(),
        unimplemented
    );
    let drained = f.trace.drain();
    let names: Vec<_> = drained
        .records
        .iter()
        .filter_map(|record| f.trace.stub_name(&record.event))
        .collect();
    assert_eq!(names, ["SetMemoryAttribute"]);
}

#[test]
fn query_memory_describes_a_mapped_region_and_a_free_gap() {
    let mut f = fixture().unwrap();
    f.mem.map(va(0x10_0000), 2 * PAGE, RW).unwrap();
    f.mem
        .map(va(0x20_0000), PAGE, Permission::READ | Permission::EXECUTE)
        .unwrap();
    f.mem.map(va(0x30_0000), PAGE, RW).unwrap();
    let out = 0x30_0000;

    let (_, regs) = f.svc(0x06, &[out, 0, 0x10_0000 + PAGE + 4]).unwrap();
    assert_eq!((regs[0], regs[1]), (0, 0));
    assert_eq!(f.mem.read::<u64>(va(out)).unwrap(), 0x10_0000);
    assert_eq!(f.mem.read::<u64>(va(out + 8)).unwrap(), 2 * PAGE);
    assert_eq!(f.mem.read::<u32>(va(out + 0x10)).unwrap(), 0x37E_BD05);
    assert_eq!(f.mem.read::<u32>(va(out + 0x18)).unwrap(), 3);

    assert_eq!(f.result(0x06, &[out, 0, 0x20_0000]).unwrap(), 0);
    assert_eq!(f.mem.read::<u32>(va(out + 0x10)).unwrap(), 0xDC_7E03);
    assert_eq!(f.mem.read::<u32>(va(out + 0x18)).unwrap(), 5);

    assert_eq!(f.result(0x06, &[out, 0, 0x10_0000 + 2 * PAGE]).unwrap(), 0);
    assert_eq!(f.mem.read::<u64>(va(out)).unwrap(), 0x10_0000 + 2 * PAGE);
    assert_eq!(
        f.mem.read::<u64>(va(out + 8)).unwrap(),
        0x20_0000 - 0x10_0000 - 2 * PAGE
    );
    assert_eq!(f.mem.read::<u32>(va(out + 0x10)).unwrap(), 0);
    assert_eq!(f.mem.read::<u32>(va(out + 0x18)).unwrap(), 0);
}

#[test]
fn query_memory_reports_a_bad_output_pointer() {
    let mut f = fixture().unwrap();
    assert_eq!(f.result(0x06, &[0x10_0000, 0, 0]).unwrap(), 0xCC01);
}

#[test]
fn query_memory_past_the_space_reports_one_inaccessible_region() {
    let mut f = fixture().unwrap();
    f.mem.map(va(0x10_0000), PAGE, RW).unwrap();
    let end = 1u64 << 39;

    assert_eq!(f.result(0x06, &[0x10_0000, 0, end + 0x1234]).unwrap(), 0);
    assert_eq!(f.mem.read::<u64>(va(0x10_0000)).unwrap(), end);
    assert_eq!(
        f.mem.read::<u64>(va(0x10_0008)).unwrap(),
        0u64.wrapping_sub(end)
    );
    assert_eq!(f.mem.read::<u32>(va(0x10_0010)).unwrap(), 0x10);
    assert_eq!(f.mem.read::<u32>(va(0x10_0018)).unwrap(), 0);
}

#[test]
fn exit_process_ends_the_guest() {
    let mut f = fixture().unwrap();
    let (flow, _) = f.svc(0x07, &[]).unwrap();
    assert_eq!(flow, Flow::Exit);
}

#[test]
fn output_debug_string_collects_the_guest_bytes() {
    let mut f = fixture().unwrap();
    f.mem.map(va(0x10_0000), PAGE, RW).unwrap();
    f.mem.write_bytes(va(0x10_0000), b"hello, B1\n").unwrap();

    assert_eq!(f.result(0x27, &[0x10_0000, 10]).unwrap(), 0);
    assert_eq!(f.result(0x27, &[0x10_0000, 0]).unwrap(), 0);
    assert_eq!(f.kernel.take_debug_output(), b"hello, B1\n");
    assert_eq!(f.kernel.take_debug_output(), Vec::<u8>::new());

    assert_eq!(f.result(0x27, &[0x10_0000, 2 * PAGE]).unwrap(), 0xCC01);
    assert_eq!(f.result(0x27, &[0x10_0000, 0x2_0000]).unwrap(), 0xCA01);
    assert_eq!(f.result(0x27, &[0x90_0000, 4]).unwrap(), 0xCC01);
    assert_eq!(f.kernel.take_debug_output(), Vec::<u8>::new());
}

#[test]
fn undrained_debug_output_is_bounded() {
    let mut f = fixture().unwrap();
    f.mem.map(va(0x10_0000), 0x1_0000, RW).unwrap();
    f.mem.write_bytes(va(0x10_0000), b"x").unwrap();

    for _ in 0..16 {
        assert_eq!(f.result(0x27, &[0x10_0000, 0x1_0000]).unwrap(), 0);
    }
    assert_eq!(f.result(0x27, &[0x10_0000, 1]).unwrap(), 0xD001);
    let output = f.kernel.take_debug_output();
    assert_eq!(output.len(), 0x10_0000);
    assert_eq!(output.first(), Some(&b'x'));

    assert_eq!(f.result(0x27, &[0x10_0000, 1]).unwrap(), 0);
    assert_eq!(f.kernel.take_debug_output(), b"x");
}

#[test]
fn break_without_the_return_bit_stops_the_guest() {
    let mut f = fixture().unwrap();
    let (flow, _) = f.svc(0x26, &[0x1, 0, 0]).unwrap();
    assert_eq!(flow, Flow::Break { reason: 0x1 });
}

#[test]
fn break_with_the_return_bit_returns_and_is_logged_once_by_name() {
    let mut f = fixture().unwrap();
    assert_eq!(f.result(0x26, &[0x8000_0000, 0, 0]).unwrap(), 0);
    assert_eq!(f.result(0x26, &[0x8000_0001, 0, 0]).unwrap(), 0);
    let (flow, _) = f.svc(0x26, &[0, 0, 0]).unwrap();
    assert_eq!(flow, Flow::Break { reason: 0 });

    let drained = f.trace.drain();
    let names: Vec<_> = drained
        .records
        .iter()
        .filter_map(|record| f.trace.stub_name(&record.event))
        .collect();
    assert_eq!(names, ["Break"]);
}

#[test]
fn an_unknown_svc_returns_a_distinct_error_and_is_logged_once() {
    let mut f = fixture().unwrap();
    let mut regs: Regs = [0xAA; 8];

    for _ in 0..3 {
        let error = f.kernel.dispatch(&mut f.mem, 0x55, &mut regs).unwrap_err();
        assert_eq!(error, KernelError::UnknownSvc { number: 0x55 });
    }
    assert_eq!(regs, [0xAA; 8]);
    let error = f.kernel.dispatch(&mut f.mem, 0x81, &mut regs).unwrap_err();
    assert_eq!(error, KernelError::UnknownSvc { number: 0x81 });

    let logged: Vec<_> = f
        .trace
        .drain()
        .records
        .iter()
        .map(|record| (record.event.kind, record.event.arg))
        .collect();
    assert_eq!(
        logged,
        [(EventKind::UnknownSvc, 0x55), (EventKind::UnknownSvc, 0x81)]
    );
}

#[test]
fn the_switch_2_svc_0x80_is_reserved_by_name_and_never_succeeds() {
    let mut f = fixture().unwrap();
    let mut regs: Regs = [0; 8];

    for _ in 0..2 {
        let error = f.kernel.dispatch(&mut f.mem, 0x80, &mut regs).unwrap_err();
        assert_eq!(
            error,
            KernelError::Unimplemented {
                number: 0x80,
                name: "SetGpuMemoryAttribute"
            }
        );
    }
    let drained = f.trace.drain();
    let names: Vec<_> = drained
        .records
        .iter()
        .filter_map(|record| f.trace.stub_name(&record.event))
        .collect();
    assert_eq!(names, ["SetGpuMemoryAttribute"]);
}

#[test]
fn svc_names_cover_the_table() {
    assert_eq!(svc_name(0x01), Some("SetHeapSize"));
    assert_eq!(svc_name(0x06), Some("QueryMemory"));
    assert_eq!(svc_name(0x27), Some("OutputDebugString"));
    assert_eq!(svc_name(0x80), Some("SetGpuMemoryAttribute"));
    assert_eq!(svc_name(0x00), None);
    assert_eq!(svc_name(0xFF), None);
}
