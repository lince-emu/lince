// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Loading valid NROs and rejecting corrupt ones through the public API (QUAL-73).
//!
//! The files are built here, field by field, from docs/research/nro-and-b1-svcs.md N2-N3. The
//! T-029 test programs are not built yet; when they are, a test loads them too.

use lince_common::GuestVa;
use lince_loader::{LoaderError, NroHeader, Segment, load_nro};
use lince_mem::{AddressSpace, MemError, Permission, SpaceConfig};

const PAGE: u32 = 0x1000;
const BASE: u64 = 0x800_0000;
const RW: Permission = Permission::READ.union(Permission::WRITE);
const RX: Permission = Permission::READ.union(Permission::EXECUTE);

/// Segment sizes in pages; each segment is filled with its own byte value.
struct Layout {
    text: u32,
    ro: u32,
    data: u32,
    bss: u32,
}

fn build(layout: &Layout) -> Vec<u8> {
    let text = layout.text * PAGE;
    let ro = layout.ro * PAGE;
    let data = layout.data * PAGE;
    let size = text + ro + data;
    let mut file = vec![0xAA; text as usize];
    file.extend(std::iter::repeat_n(0xBB, ro as usize));
    file.extend(std::iter::repeat_n(0xCC, data as usize));
    file[..0x90].fill(0);
    file[..4].copy_from_slice(&0x1400_0024_u32.to_le_bytes()); // b +0x90
    file[0x10..0x14].copy_from_slice(b"NRO0");
    let fields = [
        (0x18, size),
        (0x20, 0),
        (0x24, text),
        (0x28, text),
        (0x2C, ro),
        (0x30, text + ro),
        (0x34, data),
        (0x38, layout.bss),
    ];
    for (at, value) in fields {
        set_word(&mut file, at, value);
    }
    file
}

fn set_word(file: &mut [u8], at: usize, value: u32) {
    file[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn standard() -> Vec<u8> {
    build(&Layout {
        text: 2,
        ro: 1,
        data: 1,
        bss: 0x1801,
    })
}

fn space() -> lince_mem::Result<AddressSpace> {
    AddressSpace::new(SpaceConfig::for_host())
}

fn va(raw: u64) -> GuestVa {
    GuestVa::new(raw)
}

#[test]
fn the_header_fields_are_decoded() {
    let header = NroHeader::parse(&standard()).unwrap();

    assert_eq!(header.size, 0x4000);
    assert_eq!(header.version, 0);
    assert!(!header.flags.aligned_header);
    assert_eq!(
        header.text,
        Segment {
            offset: 0,
            size: 0x2000
        }
    );
    assert_eq!(
        header.ro,
        Segment {
            offset: 0x2000,
            size: 0x1000
        }
    );
    assert_eq!(
        header.data,
        Segment {
            offset: 0x3000,
            size: 0x1000
        }
    );
    assert_eq!(header.bss_size, 0x1801);
    assert_eq!(header.image_size(), 0x6000);
}

#[test]
fn segments_are_mapped_with_their_contents_and_permissions() {
    let mut mem = space().unwrap();

    let loaded = load_nro(&mut mem, va(BASE), &standard()).unwrap();

    assert_eq!(loaded.entry, va(BASE));
    assert_eq!(loaded.end, va(BASE + 0x6000));
    assert_eq!(mem.read::<u32>(va(BASE)).unwrap(), 0x1400_0024);
    assert_eq!(mem.read::<u8>(va(BASE + 0x1FFF)).unwrap(), 0xAA);
    assert_eq!(mem.read::<u8>(va(BASE + 0x2000)).unwrap(), 0xBB);
    assert_eq!(mem.read::<u8>(va(BASE + 0x3FFF)).unwrap(), 0xCC);
    assert_eq!(mem.read::<u64>(va(BASE + 0x5FF8)).unwrap(), 0);
    assert_eq!(mem.permission(va(BASE)), Some(RX));
    assert_eq!(mem.permission(va(BASE + 0x2000)), Some(Permission::READ));
    assert_eq!(mem.permission(va(BASE + 0x3000)), Some(RW));
    assert_eq!(mem.permission(va(BASE + 0x5000)), Some(RW));
    assert_eq!(mem.permission(va(BASE + 0x6000)), None);
}

#[test]
fn text_cannot_be_written_once_loaded() {
    let mut mem = space().unwrap();
    load_nro(&mut mem, va(BASE), &standard()).unwrap();

    let write = mem.write(va(BASE), 0_u32);

    assert!(matches!(write, Err(MemError::PermissionDenied { .. })));
}

#[test]
fn empty_segments_and_trailing_bytes_are_allowed() {
    let mut file = build(&Layout {
        text: 1,
        ro: 0,
        data: 0,
        bss: 0,
    });
    file.extend_from_slice(b"ASET and anything after the NRO");
    let mut mem = space().unwrap();

    let loaded = load_nro(&mut mem, va(BASE), &file).unwrap();

    assert_eq!(loaded.end, va(BASE + 0x1000));
    assert_eq!(mem.permission(va(BASE + 0x1000)), None);
}

#[test]
fn truncated_files_are_rejected() {
    let file = standard();

    for len in [0, 0x8F, 0x90, 0x3FFF] {
        let error = NroHeader::parse(&file[..len]).unwrap_err();
        assert!(
            matches!(error, LoaderError::Truncated { .. }),
            "len {len:#x}: {error}"
        );
    }
}

#[test]
fn a_bad_magic_is_rejected() {
    let mut file = standard();
    file[0x13] = b'1';

    let error = NroHeader::parse(&file).unwrap_err();

    assert!(matches!(error, LoaderError::BadMagic { found } if &found == b"NRO1"));
}

#[test]
fn compressed_and_unknown_flags_are_rejected() {
    for raw in [2, 1 << 31] {
        let mut file = standard();
        set_word(&mut file, 0x1C, raw);

        let error = NroHeader::parse(&file).unwrap_err();

        assert!(matches!(error, LoaderError::UnsupportedFlags { raw: r } if r == raw));
    }
    let mut file = standard();
    set_word(&mut file, 0x1C, 1);
    assert!(NroHeader::parse(&file).unwrap().flags.aligned_header);
}

#[test]
fn bad_segments_are_rejected() {
    let cases: [(usize, u32, &str); 6] = [
        (0x20, PAGE, "text not at offset 0"),
        (0x24, 0, "empty text"),
        (0x2C, 0x800, "misaligned ro size"),
        (0x28, 0x1000, "ro overlapping text"),
        (0x34, 0x2000, "data past the end"),
        (0x30, !0xFFF_u32, "data offset past the end"),
    ];
    for (at, value, what) in cases {
        let mut file = standard();
        set_word(&mut file, at, value);

        let error = NroHeader::parse(&file).unwrap_err();

        assert!(
            matches!(error, LoaderError::BadSegment { .. }),
            "{what}: {error}"
        );
    }
}

#[test]
fn a_failed_load_leaves_memory_as_it_was() {
    let mut mem = space().unwrap();
    mem.map(va(BASE + 0x5000), 0x1000, RW).unwrap();

    let error = load_nro(&mut mem, va(BASE), &standard()).unwrap_err();

    assert!(matches!(error, LoaderError::Mem(MemError::Overlap { .. })));
    for offset in (0..0x5000).step_by(0x1000) {
        assert_eq!(mem.permission(va(BASE + offset)), None, "{offset:#x}");
    }
}

#[test]
fn bases_that_do_not_fit_are_rejected() {
    let mut mem = space().unwrap();

    let misaligned = load_nro(&mut mem, va(BASE + 0x10), &standard()).unwrap_err();
    let past_space = load_nro(&mut mem, va(1 << 39), &standard()).unwrap_err();
    let wraps = load_nro(&mut mem, va(!0xFFF_u64), &standard()).unwrap_err();

    assert!(matches!(
        misaligned,
        LoaderError::Mem(MemError::Misaligned { .. })
    ));
    assert!(matches!(
        past_space,
        LoaderError::Mem(MemError::OutOfRange { .. })
    ));
    assert!(matches!(wraps, LoaderError::BadBase { .. }));
}

#[test]
fn corrupting_any_header_byte_never_panics() {
    let original = standard();
    for at in 0..0x90 {
        for value in [0x00, 0x01, 0x10, 0x7F, 0x80, 0xFF] {
            let mut file = original.clone();
            file[at] = value;
            let mut mem = space().unwrap();
            if let Ok(loaded) = load_nro(&mut mem, va(BASE), &file) {
                assert!(loaded.end.get() > BASE);
            }
        }
    }
}
