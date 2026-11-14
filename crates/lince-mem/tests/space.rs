// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Mapped, unmapped and permission-faulting accesses through the public API (QUAL-73).

use lince_common::GuestVa;
use lince_mem::{Access, AddressSpace, GUEST_PAGE_SIZE, MemError, Permission, SpaceConfig};

const PAGE: u64 = GUEST_PAGE_SIZE;
const RW: Permission = Permission::READ.union(Permission::WRITE);

fn va(raw: u64) -> GuestVa {
    GuestVa::new(raw)
}

fn space() -> lince_mem::Result<AddressSpace> {
    AddressSpace::new(SpaceConfig::for_host())
}

#[test]
fn mapped_memory_starts_zeroed_and_keeps_written_values() {
    let mut mem = space().unwrap();
    mem.map(va(0x8000_0000), 2 * PAGE, RW).unwrap();

    assert_eq!(mem.read::<u64>(va(0x8000_0010)).unwrap(), 0);
    mem.write(va(0x8000_0010), 0x1122_3344_5566_7788_u64)
        .unwrap();

    assert_eq!(
        mem.read::<u64>(va(0x8000_0010)).unwrap(),
        0x1122_3344_5566_7788
    );
    assert_eq!(mem.read::<u8>(va(0x8000_0010)).unwrap(), 0x88);
    assert_eq!(mem.read::<u16>(va(0x8000_0016)).unwrap(), 0x1122);
}

#[test]
fn an_access_across_two_adjacent_regions_succeeds() {
    let mut mem = space().unwrap();
    mem.map(va(0x1_0000), PAGE, RW).unwrap();
    mem.map(va(0x1_1000), PAGE, RW).unwrap();

    mem.write(va(0x1_0FFE), 0xAABB_CCDD_u32).unwrap();

    assert_eq!(mem.read::<u32>(va(0x1_0FFE)).unwrap(), 0xAABB_CCDD);
}

#[test]
fn the_top_of_the_39_bit_space_is_usable() {
    let mut mem = space().unwrap();
    let last = (1 << 39) - PAGE;
    mem.map(va(last), PAGE, RW).unwrap();

    mem.write(va(last + PAGE - 8), u64::MAX).unwrap();

    assert_eq!(mem.read::<u64>(va(last + PAGE - 8)).unwrap(), u64::MAX);
    assert!(matches!(
        mem.map(va(1 << 39), PAGE, RW),
        Err(MemError::OutOfRange { .. })
    ));
}

#[test]
fn reading_unmapped_memory_faults_at_the_first_unmapped_byte() {
    let mut mem = space().unwrap();
    mem.map(va(0x2_0000), PAGE, RW).unwrap();

    let fault = mem.read::<u64>(va(0x2_0FFC)).unwrap_err();

    assert!(matches!(
        fault,
        MemError::Unmapped { va: at, access: Access::Read } if at == va(0x2_1000)
    ));
    assert!(matches!(
        mem.write(va(0x9000), 1_u8),
        Err(MemError::Unmapped {
            access: Access::Write,
            ..
        })
    ));
}

#[test]
fn accesses_the_permission_forbids_fault_and_leave_memory_unchanged() {
    let mut mem = space().unwrap();
    mem.map(va(0x3_0000), PAGE, Permission::READ | Permission::EXECUTE)
        .unwrap();
    mem.map(va(0x4_0000), PAGE, Permission::NONE).unwrap();

    let write = mem.write(va(0x3_0000), 7_u32);
    let read = mem.read::<u32>(va(0x4_0000));

    assert!(matches!(
        write,
        Err(MemError::PermissionDenied {
            access: Access::Write,
            ..
        })
    ));
    assert!(matches!(
        read,
        Err(MemError::PermissionDenied {
            access: Access::Read,
            ..
        })
    ));
    assert_eq!(mem.read::<u32>(va(0x3_0000)).unwrap(), 0);
}

#[test]
fn unmapped_memory_faults_and_is_zero_when_mapped_again() {
    let mut mem = space().unwrap();
    mem.map(va(0x5_0000), PAGE, RW).unwrap();
    mem.write(va(0x5_0000), 0xFF_u8).unwrap();

    mem.unmap(va(0x5_0000), PAGE).unwrap();
    let fault = mem.read::<u8>(va(0x5_0000));
    mem.map(va(0x5_0000), PAGE, RW).unwrap();

    assert!(matches!(fault, Err(MemError::Unmapped { .. })));
    assert_eq!(mem.read::<u8>(va(0x5_0000)).unwrap(), 0);
}

#[test]
fn invalid_ranges_are_rejected() {
    let mut mem = space().unwrap();
    mem.map(va(0x6_0000), 2 * PAGE, RW).unwrap();

    assert!(matches!(
        mem.map(va(0x6_0800), PAGE, RW),
        Err(MemError::Misaligned { .. })
    ));
    assert!(matches!(
        mem.map(va(0x7_0000), 0, RW),
        Err(MemError::Misaligned { .. })
    ));
    assert!(matches!(
        mem.map(va(0x6_1000), PAGE, RW),
        Err(MemError::Overlap { .. })
    ));
    assert!(matches!(
        mem.unmap(va(0x6_0000), PAGE),
        Err(MemError::NotARegion { .. })
    ));
}

#[test]
fn with_16_kib_host_pages_unmapping_keeps_neighbours_in_the_same_host_page() {
    let config = SpaceConfig {
        address_bits: 39,
        host_page_size: 0x4000,
    };
    let mut mem = AddressSpace::new(config).unwrap();
    // Four guest pages share the host page at 0x10000; a fifth starts the next one.
    mem.map(va(0x1_0000), PAGE, RW).unwrap();
    mem.map(va(0x1_1000), 4 * PAGE, RW).unwrap();
    mem.write(va(0x1_1000), 0x55_u8).unwrap();
    mem.write(va(0x1_4000), 0x66_u8).unwrap();

    mem.unmap(va(0x1_0000), PAGE).unwrap();
    mem.map(va(0x1_0000), PAGE, RW).unwrap();

    assert_eq!(mem.read::<u8>(va(0x1_0000)).unwrap(), 0);
    assert_eq!(mem.read::<u8>(va(0x1_1000)).unwrap(), 0x55);
    assert_eq!(mem.read::<u8>(va(0x1_4000)).unwrap(), 0x66);
    mem.unmap(va(0x1_1000), 4 * PAGE).unwrap();
    assert_eq!(mem.read::<u8>(va(0x1_0000)).unwrap(), 0);
}

#[test]
fn unusable_configurations_are_rejected() {
    let bad_page = SpaceConfig {
        address_bits: 39,
        host_page_size: 0x3000,
    };
    let too_wide = SpaceConfig {
        address_bits: 64,
        host_page_size: 0x4000,
    };

    assert!(matches!(
        AddressSpace::new(bad_page),
        Err(MemError::InvalidConfig { .. })
    ));
    assert!(matches!(
        AddressSpace::new(too_wide),
        Err(MemError::InvalidConfig { .. })
    ));
}
