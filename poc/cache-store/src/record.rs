// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Segment record framing (docs/02-cache-architecture.md §4.1).
//!
//! ```text
//! 0   4  magic "LNXR"        16 16 key (u128 LE)
//! 4   2  kind                32  4 header CRC-32C over bytes 0..32
//! 6   1  codec (0 = raw)     36  4 reserved (0)
//! 7   1  flags               40  8 payload checksum (PoC: CRC-32C seeded with kind and key)
//! 8   4  stored_len          48    payload, then zero padding to 16-byte alignment
//! 12  4  raw_len
//! ```

use crate::crc32c;

/// Size of the record header in bytes.
pub(crate) const HEADER_LEN: usize = 48;
/// Records start at multiples of this many bytes.
pub(crate) const ALIGN: u64 = 16;
const MAGIC: [u8; 4] = *b"LNXR";

/// A validated record header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) kind: u16,
    pub(crate) stored_len: u32,
    pub(crate) key: u128,
    pub(crate) payload_check: u64,
}

/// Rounds `n` up to the record alignment.
pub(crate) fn align_up(n: u64) -> u64 {
    n.div_ceil(ALIGN) * ALIGN
}

/// Checksum of a payload, seeded with its kind and key so a payload moved to another record
/// does not validate.
pub(crate) fn payload_check(kind: u16, key: u128, payload: &[u8]) -> u64 {
    let seed = crc32c::update(crc32c::update(0, &key.to_le_bytes()), &kind.to_le_bytes());
    u64::from(crc32c::update(seed, payload))
}

/// Encodes a header for `payload`. The caller checked that the length fits in `u32`.
pub(crate) fn encode(kind: u16, key: u128, payload: &[u8], stored_len: u32) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0..4].copy_from_slice(&MAGIC);
    h[4..6].copy_from_slice(&kind.to_le_bytes());
    h[6] = 0; // codec: raw
    h[7] = 0; // flags
    h[8..12].copy_from_slice(&stored_len.to_le_bytes());
    h[12..16].copy_from_slice(&stored_len.to_le_bytes());
    h[16..32].copy_from_slice(&key.to_le_bytes());
    let crc = crc32c::checksum(&h[0..32]);
    h[32..36].copy_from_slice(&crc.to_le_bytes());
    h[40..48].copy_from_slice(&payload_check(kind, key, payload).to_le_bytes());
    h
}

/// Decodes a header. Returns `None` for anything that does not validate: a torn or corrupt
/// header is a miss, never an error.
pub(crate) fn decode(h: &[u8; HEADER_LEN]) -> Option<Header> {
    let stored_crc = u32::from_le_bytes([h[32], h[33], h[34], h[35]]);
    if h[0..4] != MAGIC || crc32c::checksum(&h[0..32]) != stored_crc {
        return None;
    }
    if h[6] != 0 || h[36..40] != [0; 4] {
        return None; // compressed records and reserved bits are not produced by this PoC
    }
    let mut key = [0u8; 16];
    key.copy_from_slice(&h[16..32]);
    let mut check = [0u8; 8];
    check.copy_from_slice(&h[40..48]);
    Some(Header {
        kind: u16::from_le_bytes([h[4], h[5]]),
        stored_len: u32::from_le_bytes([h[8], h[9], h[10], h[11]]),
        key: u128::from_le_bytes(key),
        payload_check: u64::from_le_bytes(check),
    })
}

#[cfg(test)]
mod tests {
    use super::{HEADER_LEN, align_up, decode, encode, payload_check};

    #[test]
    fn header_round_trips() {
        let payload = b"payload bytes";
        let h = encode(3, 0xABCD, payload, 13);
        let d = decode(&h).expect("valid header");
        assert_eq!(d.kind, 3);
        assert_eq!(d.stored_len, 13);
        assert_eq!(d.key, 0xABCD);
        assert_eq!(d.payload_check, payload_check(3, 0xABCD, payload));
    }

    #[test]
    fn any_single_bit_flip_in_the_checked_bytes_is_rejected() {
        let h = encode(1, 42, b"x", 1);
        for bit in 0..(36 * 8) {
            let mut bad: [u8; HEADER_LEN] = h;
            bad[bit / 8] ^= 1 << (bit % 8);
            assert!(decode(&bad).is_none(), "flip of bit {bit} was accepted");
        }
    }

    #[test]
    fn payload_check_depends_on_key_and_kind() {
        let p = b"same bytes";
        assert_ne!(payload_check(1, 1, p), payload_check(1, 2, p));
        assert_ne!(payload_check(1, 1, p), payload_check(2, 1, p));
    }

    #[test]
    fn align_up_rounds_to_sixteen() {
        assert_eq!(align_up(0), 0);
        assert_eq!(align_up(1), 16);
        assert_eq!(align_up(48), 48);
        assert_eq!(align_up(49), 64);
    }
}
