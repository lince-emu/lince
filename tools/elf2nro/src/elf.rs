// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads the load segments of an ELF-64 file (offsets from the public ELF-64 specification).

use crate::error::{ConvertError, Result};

const MAGIC: [u8; 4] = *b"\x7fELF";
const CLASS_64: u8 = 2;
const DATA_LITTLE_ENDIAN: u8 = 1;
const MACHINE_AARCH64: u16 = 183;
const PROGRAM_HEADER_SIZE: usize = 56;
const PT_LOAD: u32 = 1;
/// Segment flag bits.
pub(crate) const PF_X: u32 = 1;
pub(crate) const PF_W: u32 = 2;

/// One `PT_LOAD` program header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoadSegment {
    pub(crate) flags: u32,
    pub(crate) vaddr: u64,
    /// The segment's bytes in the file (`p_filesz` long).
    pub(crate) file_range: (usize, usize),
    pub(crate) mem_size: u64,
}

/// The `PT_LOAD` segments of `elf`, in program-header order.
pub(crate) fn load_segments(elf: &[u8]) -> Result<Vec<LoadSegment>> {
    let ident = elf
        .get(..0x40)
        .ok_or(ConvertError::BadElf("shorter than the ELF header"))?;
    if ident[..4] != MAGIC || ident[4] != CLASS_64 || ident[5] != DATA_LITTLE_ENDIAN {
        return Err(ConvertError::BadElf("not little-endian ELF64"));
    }
    if u16_at(elf, 0x12)? != MACHINE_AARCH64 {
        return Err(ConvertError::BadElf("not AArch64"));
    }
    let table = to_usize(u64_at(elf, 0x20)?)?;
    let entry_size = usize::from(u16_at(elf, 0x36)?);
    let count = usize::from(u16_at(elf, 0x38)?);
    if entry_size != PROGRAM_HEADER_SIZE {
        return Err(ConvertError::BadElf("unexpected program header size"));
    }
    let mut segments = Vec::new();
    for index in 0..count {
        let at = table + index * PROGRAM_HEADER_SIZE;
        if u32_at(elf, at)? != PT_LOAD {
            continue;
        }
        let offset = to_usize(u64_at(elf, at + 0x08)?)?;
        let file_size = to_usize(u64_at(elf, at + 0x20)?)?;
        let end = offset
            .checked_add(file_size)
            .filter(|end| *end <= elf.len())
            .ok_or(ConvertError::BadElf("segment runs past the file"))?;
        segments.push(LoadSegment {
            flags: u32_at(elf, at + 0x04)?,
            vaddr: u64_at(elf, at + 0x10)?,
            file_range: (offset, end),
            mem_size: u64_at(elf, at + 0x28)?,
        });
    }
    Ok(segments)
}

fn bytes_at<const N: usize>(elf: &[u8], at: usize) -> Result<[u8; N]> {
    at.checked_add(N)
        .and_then(|end| elf.get(at..end))
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(ConvertError::BadElf("table runs past the file"))
}

fn u16_at(elf: &[u8], at: usize) -> Result<u16> {
    bytes_at(elf, at).map(u16::from_le_bytes)
}

fn u32_at(elf: &[u8], at: usize) -> Result<u32> {
    bytes_at(elf, at).map(u32::from_le_bytes)
}

fn u64_at(elf: &[u8], at: usize) -> Result<u64> {
    bytes_at(elf, at).map(u64::from_le_bytes)
}

fn to_usize(value: u64) -> Result<usize> {
    usize::try_from(value).map_err(|_| ConvertError::BadElf("offset does not fit in memory"))
}
