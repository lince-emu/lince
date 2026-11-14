// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds the NRO image from the ELF load segments.

use crate::elf::{self, LoadSegment, PF_W, PF_X};
use crate::error::{ConvertError, Result};

const PAGE: u64 = 0x1000;
/// File offset of the `NRO0` header and one past its end (research N1-N2).
const HEADER_OFFSET: usize = 0x10;
const HEADER_END: usize = HEADER_OFFSET + 0x80;
const MAGIC: [u8; 4] = *b"NRO0";

/// One NRO segment: offset from the image start and size, both whole pages.
#[derive(Clone, Copy)]
struct Placed {
    offset: u64,
    size: u64,
}

impl Placed {
    fn end(self) -> u64 {
        self.offset + self.size
    }
}

/// Converts a statically linked `AArch64` ELF laid out by `tests/homebrew/nro.ld` into NRO bytes.
pub fn convert(elf: &[u8]) -> Result<Vec<u8>> {
    let segments = elf::load_segments(elf)?;
    let [text, ro, data] = classify(&segments)?;
    let text = text.ok_or(ConvertError::BadLayout("no executable segment"))?;
    if text.vaddr != 0 {
        return Err(ConvertError::BadLayout("text must start at address 0"));
    }
    let start_block = &elf[text.file_range.0..text.file_range.1];
    if start_block
        .get(HEADER_OFFSET..HEADER_END)
        .is_none_or(|header| header.iter().any(|b| *b != 0))
    {
        return Err(ConvertError::BadLayout(
            "text does not reserve zeroed header bytes",
        ));
    }
    let text_placed = place(text, 0, false)?;
    let ro_placed = place_optional(ro, text_placed.end(), false)?;
    let data_placed = place_optional(data, ro_placed.end(), true)?;
    let bss = data.map_or(0, |segment| {
        segment.mem_size.saturating_sub(data_placed.size)
    });
    let mut image =
        vec![0; usize::try_from(data_placed.end()).map_err(|_| ConvertError::TooLarge)?];
    for segment in [Some(text), ro, data].into_iter().flatten() {
        let bytes = &elf[segment.file_range.0..segment.file_range.1];
        let at = usize::try_from(segment.vaddr).map_err(|_| ConvertError::TooLarge)?;
        image[at..at + bytes.len()].copy_from_slice(bytes);
    }
    let fields = [
        (0x00, 0),
        (0x04, field(data_placed.end())?),
        (0x08, 0),
        (0x0C, field(text_placed.offset)?),
        (0x10, field(text_placed.size)?),
        (0x14, field(ro_placed.offset)?),
        (0x18, field(ro_placed.size)?),
        (0x1C, field(data_placed.offset)?),
        (0x20, field(data_placed.size)?),
        (0x24, field(bss)?),
    ];
    image[HEADER_OFFSET..HEADER_OFFSET + 4].copy_from_slice(&MAGIC);
    for (at, value) in fields {
        let at = HEADER_OFFSET + 4 + at;
        image[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(image)
}

/// Sorts the load segments into text (executable), ro (read-only) and data (writable).
fn classify(segments: &[LoadSegment]) -> Result<[Option<LoadSegment>; 3]> {
    let mut slots = [None; 3];
    for segment in segments {
        let slot = match (segment.flags & PF_X != 0, segment.flags & PF_W != 0) {
            (true, true) => return Err(ConvertError::BadLayout("writable and executable segment")),
            (true, false) => 0,
            (false, false) => 1,
            (false, true) => 2,
        };
        if slots[slot].replace(*segment).is_some() {
            return Err(ConvertError::BadLayout("more than one segment of a kind"));
        }
    }
    Ok(slots)
}

/// Places `segment` at its own address, which must be page-aligned and at or after `after`.
fn place(segment: LoadSegment, after: u64, allow_bss: bool) -> Result<Placed> {
    let file_size = (segment.file_range.1 - segment.file_range.0) as u64;
    if !segment.vaddr.is_multiple_of(PAGE) || segment.vaddr < after {
        return Err(ConvertError::BadLayout(
            "segment misaligned or out of order",
        ));
    }
    if !allow_bss && segment.mem_size != file_size {
        return Err(ConvertError::BadLayout(
            "zero-filled bytes outside the data segment",
        ));
    }
    Ok(Placed {
        offset: segment.vaddr,
        size: file_size.div_ceil(PAGE) * PAGE,
    })
}

/// Like [`place`]; an absent segment is empty and sits at `after`.
fn place_optional(segment: Option<LoadSegment>, after: u64, allow_bss: bool) -> Result<Placed> {
    segment.map_or(
        Ok(Placed {
            offset: after,
            size: 0,
        }),
        |segment| place(segment, after, allow_bss),
    )
}

fn field(value: u64) -> Result<u32> {
    u32::try_from(value).map_err(|_| ConvertError::TooLarge)
}
