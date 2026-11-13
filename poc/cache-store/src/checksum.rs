// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Payload checksum algorithms the harness compares (T-020, decision D-001).
//!
//! Records use XXH3-64 for payloads and CRC-32C for headers, as
//! docs/02-cache-architecture.md §4.1 specifies. This module exposes both so the harness can
//! time them on the same payloads.

use crate::{crc32c, record};

// Any kind and key work for timing: the cost of the seeded call does not depend on them.
/// Record kind used when timing the record's XXH3-64 path.
const TIMING_KIND: u16 = 1;
/// Record key used when timing the record's XXH3-64 path.
const TIMING_KEY: u128 = 0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210;

/// A checksum algorithm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checksum {
    /// CRC-32C, this crate's table-driven implementation (eight bytes per step).
    Crc32c,
    /// XXH3-64 from `xxhash-rust`, seeded with a record kind and key exactly as records
    /// compute their payload checksum (including deriving the seed).
    Xxh3_64,
}

impl Checksum {
    /// Every algorithm, in CSV order.
    pub const ALL: [Self; 2] = [Self::Crc32c, Self::Xxh3_64];

    /// CSV name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Crc32c => "crc32c",
            Self::Xxh3_64 => "xxh3_64",
        }
    }

    /// Checksum of `data`, widened to 64 bits.
    pub fn compute(self, data: &[u8]) -> u64 {
        match self {
            Self::Crc32c => u64::from(crc32c::checksum(data)),
            Self::Xxh3_64 => record::payload_check(TIMING_KIND, TIMING_KEY, data),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Checksum, TIMING_KEY, TIMING_KIND};
    use crate::record::payload_check;

    #[test]
    fn crc32c_matches_its_standard_check_value() {
        // The CRC-32C check value for the ASCII string "123456789".
        assert_eq!(Checksum::Crc32c.compute(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn xxh3_variant_times_the_record_payload_checksum() {
        let data = b"payload bytes";
        assert_eq!(
            Checksum::Xxh3_64.compute(data),
            payload_check(TIMING_KIND, TIMING_KEY, data)
        );
    }
}
