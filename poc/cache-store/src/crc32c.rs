// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! CRC-32C (Castagnoli polynomial, reflected), table-driven, eight bytes per step.
//!
//! Used for record headers, the manifest and the index (docs/02-cache-architecture.md §4.1).
//! Payloads use XXH3-64; the harness times both (`checksum` module).

/// Reflected Castagnoli polynomial.
const POLY: u32 = 0x82F6_3B78;

/// `TABLES[k][b]` is the CRC contribution of byte `b` followed by `k` zero bytes.
static TABLES: [[u32; 256]; 8] = build_tables();

const fn build_tables() -> [[u32; 256]; 8] {
    let mut tables = [[0u32; 256]; 8];
    let mut b: u32 = 0;
    while b < 256 {
        let mut crc = b;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        tables[0][b as usize] = crc;
        b += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut b = 0;
        while b < 256 {
            let prev = tables[k - 1][b];
            tables[k][b] = (prev >> 8) ^ tables[0][(prev & 0xFF) as usize];
            b += 1;
        }
        k += 1;
    }
    tables
}

/// Continues a CRC-32C over `data`. Start with `0` for a fresh checksum.
pub(crate) fn update(crc: u32, data: &[u8]) -> u32 {
    let mut crc = !crc;
    let (chunks, rest) = data.as_chunks::<8>();
    for chunk in chunks {
        let lo = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) ^ crc;
        crc = TABLES[7][usize::from(lo.to_le_bytes()[0])]
            ^ TABLES[6][usize::from(lo.to_le_bytes()[1])]
            ^ TABLES[5][usize::from(lo.to_le_bytes()[2])]
            ^ TABLES[4][usize::from(lo.to_le_bytes()[3])]
            ^ TABLES[3][usize::from(chunk[4])]
            ^ TABLES[2][usize::from(chunk[5])]
            ^ TABLES[1][usize::from(chunk[6])]
            ^ TABLES[0][usize::from(chunk[7])];
    }
    for &byte in rest {
        crc = (crc >> 8) ^ TABLES[0][usize::from(crc.to_le_bytes()[0] ^ byte)];
    }
    !crc
}

/// CRC-32C of `data`.
pub(crate) fn checksum(data: &[u8]) -> u32 {
    update(0, data)
}

#[cfg(test)]
mod tests {
    use super::{checksum, update};

    #[test]
    fn matches_the_standard_check_value() {
        // The CRC-32C check value for the ASCII string "123456789".
        assert_eq!(checksum(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn empty_input_is_zero() {
        assert_eq!(checksum(&[]), 0);
    }

    #[test]
    fn incremental_update_equals_one_shot() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7 + 3).to_le_bytes()[0]).collect();
        let split = update(update(0, &data[..333]), &data[333..]);
        assert_eq!(split, checksum(&data));
    }

    #[test]
    fn slicing_path_equals_bytewise_path() {
        let data: Vec<u8> = (0..64u8).collect();
        let bytewise = data
            .iter()
            .fold(0, |crc, b| update(crc, std::slice::from_ref(b)));
        assert_eq!(bytewise, checksum(&data));
    }
}
