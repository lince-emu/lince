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
fn protect_changes_what_accesses_are_allowed_and_keeps_the_contents() {
    let mut mem = space().unwrap();
    mem.map(va(0x4_8000), 2 * PAGE, RW).unwrap();
    mem.write(va(0x4_8000), 0x55_u8).unwrap();

    mem.protect(va(0x4_8000), 2 * PAGE, Permission::READ)
        .unwrap();

    assert!(matches!(
        mem.write(va(0x4_8000), 1_u8),
        Err(MemError::PermissionDenied { .. })
    ));
    assert_eq!(mem.read::<u8>(va(0x4_8000)).unwrap(), 0x55);
    assert_eq!(mem.permission(va(0x4_9000)), Some(Permission::READ));
}

/// The mapped regions in `[0x4_0000, 0x6_0000)` as (base, size, permission), walked with
/// `query`.
fn layout(mem: &AddressSpace) -> Vec<(u64, u64, Permission)> {
    let mut regions = Vec::new();
    let mut addr = 0x4_0000;
    while addr < 0x6_0000 {
        let span = mem.query(va(addr)).expect("inside the space");
        if let Some(perm) = span.permission {
            regions.push((span.base.get(), span.size, perm));
        }
        addr = span.base.get() + span.size;
    }
    regions
}

#[test]
fn protect_splits_a_region_at_its_head_middle_and_tail() {
    const R: Permission = Permission::READ;
    let base = 0x4_0000;
    let mut mem = space().unwrap();
    mem.map(va(base), 6 * PAGE, RW).unwrap();

    mem.protect(va(base), PAGE, R).unwrap();
    mem.protect(va(base + 2 * PAGE), PAGE, R).unwrap();
    mem.protect(va(base + 5 * PAGE), PAGE, R).unwrap();

    assert_eq!(
        layout(&mem),
        [
            (base, PAGE, R),
            (base + PAGE, PAGE, RW),
            (base + 2 * PAGE, PAGE, R),
            (base + 3 * PAGE, 2 * PAGE, RW),
            (base + 5 * PAGE, PAGE, R),
        ]
    );
    let middle = mem.query(va(base + 2 * PAGE)).unwrap();
    assert_eq!((middle.base, middle.size), (va(base + 2 * PAGE), PAGE));
    assert!(matches!(
        mem.write(va(base + 2 * PAGE), 1_u8),
        Err(MemError::PermissionDenied { .. })
    ));
    mem.write(va(base + 3 * PAGE), 1_u8).unwrap();
}

#[test]
fn protect_across_adjacent_regions_merges_what_ends_up_equal() {
    let mut mem = space().unwrap();
    mem.map(va(0x4_0000), 2 * PAGE, RW).unwrap();
    mem.map(va(0x4_2000), 2 * PAGE, Permission::READ).unwrap();

    mem.protect(va(0x4_1000), 2 * PAGE, Permission::READ)
        .unwrap();
    assert_eq!(
        layout(&mem),
        [(0x4_0000, PAGE, RW), (0x4_1000, 3 * PAGE, Permission::READ)]
    );

    mem.protect(va(0x4_1000), 3 * PAGE, RW).unwrap();
    assert_eq!(layout(&mem), [(0x4_0000, 4 * PAGE, RW)]);
}

#[test]
fn protect_rejects_a_range_with_an_unmapped_page_and_changes_nothing() {
    let mut mem = space().unwrap();
    mem.map(va(0x4_C000), PAGE, RW).unwrap();
    mem.map(va(0x4_E000), PAGE, RW).unwrap();
    let before = layout(&mem);

    let gap = mem.protect(va(0x4_C000), 3 * PAGE, Permission::READ);
    let unmapped = mem.protect(va(0x5_8000), PAGE, Permission::READ);

    assert!(matches!(gap, Err(MemError::NotARegion { .. })));
    assert!(matches!(unmapped, Err(MemError::NotARegion { .. })));
    assert_eq!(layout(&mem), before);
}

#[test]
fn unmap_removes_part_of_a_region_or_a_range_across_regions() {
    let mut mem = space().unwrap();
    mem.map(va(0x4_0000), 4 * PAGE, RW).unwrap();
    mem.map(va(0x4_4000), 2 * PAGE, Permission::READ).unwrap();
    mem.write(va(0x4_0000), 7_u8).unwrap();

    mem.unmap(va(0x4_1000), PAGE).unwrap();
    mem.unmap(va(0x4_3000), 2 * PAGE).unwrap();

    assert_eq!(
        layout(&mem),
        [
            (0x4_0000, PAGE, RW),
            (0x4_2000, PAGE, RW),
            (0x4_5000, PAGE, Permission::READ),
        ]
    );
    assert_eq!(mem.read::<u8>(va(0x4_0000)).unwrap(), 7);
    assert!(matches!(
        mem.read::<u8>(va(0x4_1000)),
        Err(MemError::Unmapped { .. })
    ));
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
        mem.unmap(va(0x6_1000), 2 * PAGE),
        Err(MemError::NotARegion { .. })
    ));
    assert_eq!(mem.permission(va(0x6_1000)), Some(RW));
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
fn with_16_kib_host_pages_unmapping_part_of_a_region_keeps_the_remnants() {
    let config = SpaceConfig {
        address_bits: 39,
        host_page_size: 0x4000,
    };
    let mut mem = AddressSpace::new(config).unwrap();
    // Twelve guest pages over three host pages; the unmapped range [0x2_2000, 0x2_9000) shares
    // its first and last host pages with the remnants and covers the middle one entirely.
    mem.map(va(0x2_0000), 12 * PAGE, RW).unwrap();
    mem.write(va(0x2_1000), 0x11_u8).unwrap();
    mem.write(va(0x2_5000), 0x55_u8).unwrap();
    mem.write(va(0x2_9000), 0x99_u8).unwrap();

    mem.unmap(va(0x2_2000), 7 * PAGE).unwrap();

    assert_eq!(mem.read::<u8>(va(0x2_1000)).unwrap(), 0x11);
    assert_eq!(mem.read::<u8>(va(0x2_9000)).unwrap(), 0x99);
    assert!(matches!(
        mem.read::<u8>(va(0x2_5000)),
        Err(MemError::Unmapped { .. })
    ));
    mem.map(va(0x2_4000), 4 * PAGE, RW).unwrap();
    assert_eq!(mem.read::<u8>(va(0x2_5000)).unwrap(), 0);
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

#[test]
fn query_reports_the_region_or_the_free_gap_around_an_address() {
    let mut mem = space().unwrap();
    let size = 1u64 << 39;
    mem.map(va(0x10_0000), 2 * PAGE, RW).unwrap();
    mem.map(va(0x20_0000), PAGE, Permission::READ).unwrap();

    let region = mem.query(va(0x10_0000 + PAGE + 8)).unwrap();
    assert_eq!(
        (region.base, region.size, region.permission),
        (va(0x10_0000), 2 * PAGE, Some(RW))
    );

    let before = mem.query(va(0x1000)).unwrap();
    assert_eq!(
        (before.base, before.size, before.permission),
        (va(0), 0x10_0000, None)
    );
    let between = mem.query(va(0x10_0000 + 2 * PAGE)).unwrap();
    assert_eq!(
        (between.base, between.size, between.permission),
        (
            va(0x10_0000 + 2 * PAGE),
            0x20_0000 - 0x10_0000 - 2 * PAGE,
            None
        )
    );
    let after = mem.query(va(0x30_0000)).unwrap();
    assert_eq!(
        (after.base, after.size, after.permission),
        (va(0x20_0000 + PAGE), size - 0x20_0000 - PAGE, None)
    );
    assert_eq!(mem.query(va(size)), None);
}

#[test]
fn regions_lists_every_mapped_region_in_address_order() {
    let mut mem = space().unwrap();
    mem.map(va(0x3_0000), PAGE, Permission::READ).unwrap();
    mem.map(va(0x1_0000), 2 * PAGE, RW).unwrap();
    mem.map(va(0x2_0000), PAGE, RW).unwrap();
    mem.unmap(va(0x2_0000), PAGE).unwrap();

    let regions: Vec<_> = mem
        .regions()
        .map(|s| (s.base, s.size, s.permission))
        .collect();

    assert_eq!(
        regions,
        [
            (va(0x1_0000), 2 * PAGE, Some(RW)),
            (va(0x3_0000), PAGE, Some(Permission::READ)),
        ]
    );
}

#[test]
fn host_base_is_the_same_for_the_life_of_the_space() {
    let mut mem = space().unwrap();
    let before = mem.host_base();
    mem.map(va(0x1_0000), PAGE, RW).unwrap();

    assert_eq!(mem.host_base(), before);
}

#[test]
fn permission_bits_match_the_c_header_values() {
    assert_eq!(Permission::READ.bits(), 1);
    assert_eq!(Permission::WRITE.bits(), 2);
    assert_eq!(Permission::EXECUTE.bits(), 4);
    assert_eq!((Permission::READ | Permission::EXECUTE).bits(), 5);
}
