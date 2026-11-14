// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The NRO format: header parsing and mapping into guest memory.
//!
//! Layout (docs/research/nro-and-b1-svcs.md N1-N4): a 16-byte start block, then the `NRO0`
//! header of 0x80 bytes at file offset 0x10. Segment offsets are both file offsets and offsets
//! from the load base. The file starts with a branch over the header, so execution begins at the
//! load base and the text segment must start at file offset 0.

use lince_common::GuestVa;
use lince_mem::{AddressSpace, GUEST_PAGE_SIZE, Permission};

use crate::error::{LoaderError, Result};

/// File offset of the `NRO0` header.
const HEADER_OFFSET: usize = 0x10;
/// File offset one past the header (0x80 header bytes after the start block).
const HEADER_END: usize = HEADER_OFFSET + 0x80;
const MAGIC: [u8; 4] = *b"NRO0";
/// Flag bit 0, `AlignedHeader` (N2). Its effect on loading is UNKNOWN (research O1); it is kept.
const FLAG_ALIGNED_HEADER: u32 = 1;

/// One file segment: an offset from the file start (and the load base) and a size in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    /// File offset, equal to the offset from the load base.
    pub offset: u32,
    /// Size in bytes.
    pub size: u32,
}

impl Segment {
    fn end(self) -> u64 {
        u64::from(self.offset) + u64::from(self.size)
    }
}

/// Decoded header flags. Any bit not represented here is rejected by [`NroHeader::parse`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NroFlags {
    /// Bit 0, `AlignedHeader`.
    pub aligned_header: bool,
}

/// A validated NRO header: every segment lies inside the file, is page-aligned and in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NroHeader {
    /// Format version (`+0x14`); B1 files write 0 (research O1).
    pub version: u32,
    /// Size of the NRO in bytes (`+0x18`); bytes past it (an asset block) are ignored.
    pub size: u32,
    /// Decoded flags (`+0x1C`).
    pub flags: NroFlags,
    /// Code, mapped read-execute.
    pub text: Segment,
    /// Read-only data, mapped read-only.
    pub ro: Segment,
    /// Initialized data, mapped read-write.
    pub data: Segment,
    /// Zero-filled bytes after data, mapped read-write; not stored in the file.
    pub bss_size: u32,
}

impl NroHeader {
    /// Parses and validates the header of `file`.
    pub fn parse(file: &[u8]) -> Result<Self> {
        let header = file
            .get(..HEADER_END)
            .ok_or_else(|| truncated(HEADER_END as u64, file))?;
        let word = |offset: usize| {
            let at = HEADER_OFFSET + offset;
            u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
        };
        let found = [header[0x10], header[0x11], header[0x12], header[0x13]];
        if found != MAGIC {
            return Err(LoaderError::BadMagic { found });
        }
        let raw_flags = word(0x0C);
        if raw_flags & !FLAG_ALIGNED_HEADER != 0 {
            return Err(LoaderError::UnsupportedFlags { raw: raw_flags });
        }
        let parsed = Self {
            version: word(0x04),
            size: word(0x08),
            flags: NroFlags {
                aligned_header: raw_flags & FLAG_ALIGNED_HEADER != 0,
            },
            text: Segment {
                offset: word(0x10),
                size: word(0x14),
            },
            ro: Segment {
                offset: word(0x18),
                size: word(0x1C),
            },
            data: Segment {
                offset: word(0x20),
                size: word(0x24),
            },
            bss_size: word(0x28),
        };
        parsed.validate(file)?;
        Ok(parsed)
    }

    fn validate(&self, file: &[u8]) -> Result<()> {
        let size = u64::from(self.size);
        if size > file.len() as u64 {
            return Err(truncated(size, file));
        }
        let segments = [("text", self.text), ("ro", self.ro), ("data", self.data)];
        let mut previous_end = 0;
        for (name, segment) in segments {
            let reason = if !is_page_aligned(segment.offset) || !is_page_aligned(segment.size) {
                Some("not page-aligned")
            } else if segment.end() > size {
                Some("past the end of the NRO")
            } else if u64::from(segment.offset) < previous_end {
                Some("overlaps the previous segment")
            } else {
                None
            };
            if let Some(reason) = reason {
                return Err(bad_segment(name, segment, reason));
            }
            previous_end = segment.end();
        }
        if self.text.offset != 0 || self.text.size == 0 {
            return Err(bad_segment(
                "text",
                self.text,
                "must start at offset 0 and hold the header",
            ));
        }
        Ok(())
    }

    /// Bytes from the load base to the end of bss, rounded up to whole pages.
    pub fn image_size(&self) -> u64 {
        self.data.end() + round_up_to_page(u64::from(self.bss_size))
    }
}

/// Where a loaded NRO sits in guest memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedNro {
    /// The parsed header.
    pub header: NroHeader,
    /// The load base, which is also the entry point.
    pub entry: GuestVa,
    /// One past the last mapped byte (end of bss).
    pub end: GuestVa,
}

/// Parses `file` and maps its segments and bss at `base`, which must be page-aligned.
///
/// Gaps between segments stay unmapped. On error, every region this call mapped is unmapped
/// again, so `mem` is left as it was.
pub fn load_nro(mem: &mut AddressSpace, base: GuestVa, file: &[u8]) -> Result<LoadedNro> {
    let header = NroHeader::parse(file)?;
    let image_size = header.image_size();
    let end = base.checked_add(image_size).ok_or(LoaderError::BadBase {
        base: base.get(),
        size: image_size,
    })?;
    let rx = Permission::READ | Permission::EXECUTE;
    let rw = Permission::READ | Permission::WRITE;
    let bss = Segment {
        offset: header.data.offset.saturating_add(header.data.size),
        size: 0,
    };
    let plan = [
        (header.text, u64::from(header.text.size), rx),
        (header.ro, u64::from(header.ro.size), Permission::READ),
        (header.data, u64::from(header.data.size), rw),
        (bss, round_up_to_page(u64::from(header.bss_size)), rw),
    ];
    let mut mapped: Vec<(GuestVa, u64)> = Vec::with_capacity(plan.len());
    for (segment, size, perm) in plan {
        if size == 0 {
            continue;
        }
        let va = base.wrapping_add(u64::from(segment.offset));
        if let Err(error) = map_segment(mem, va, size, perm, contents(file, segment)) {
            for (va, size) in mapped.into_iter().rev() {
                mem.unmap(va, size)?;
            }
            return Err(error);
        }
        mapped.push((va, size));
    }
    Ok(LoadedNro {
        header,
        entry: base,
        end,
    })
}

fn map_segment(
    mem: &mut AddressSpace,
    va: GuestVa,
    size: u64,
    perm: Permission,
    bytes: &[u8],
) -> Result<()> {
    mem.map(va, size, Permission::READ | Permission::WRITE)?;
    let filled = mem
        .write_bytes(va, bytes)
        .and_then(|()| mem.protect(va, size, perm));
    if let Err(error) = filled {
        mem.unmap(va, size)?;
        return Err(error.into());
    }
    Ok(())
}

/// The file bytes of a validated segment; empty for bss.
fn contents(file: &[u8], segment: Segment) -> &[u8] {
    let start = segment.offset as usize;
    file.get(start..start + segment.size as usize)
        .unwrap_or(&[])
}

fn is_page_aligned(value: u32) -> bool {
    u64::from(value).is_multiple_of(GUEST_PAGE_SIZE)
}

fn round_up_to_page(value: u64) -> u64 {
    value.div_ceil(GUEST_PAGE_SIZE) * GUEST_PAGE_SIZE
}

fn truncated(needed: u64, file: &[u8]) -> LoaderError {
    LoaderError::Truncated {
        needed,
        len: file.len() as u64,
    }
}

fn bad_segment(segment: &'static str, s: Segment, reason: &'static str) -> LoaderError {
    LoaderError::BadSegment {
        segment,
        offset: s.offset,
        size: s.size,
        reason,
    }
}
