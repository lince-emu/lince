// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The CMIF header inside the raw data of a HIPC message (docs/research/b2-svcs-and-ipc.md C3,
//! C5, C6). Domains (C7) are out of B2 scope (ADR-0024).
//!
//! The raw data holds padding up to a 16-byte boundary of the buffer, the 0x10-byte header,
//! the parameters, then the rest of the padding: 0x10 bytes of padding in total.

use crate::error::{IpcError, Result};
use crate::hipc::{self, MESSAGE_SIZE, Message, MessageBuffer, Parts};

/// Message type of a request to the session's object (C3, 5.0.0+ name `Invoke2Method`).
pub const TYPE_REQUEST: u16 = 6;
/// Message type that closes the session (C3, `Release`).
pub const TYPE_CLOSE: u16 = 2;
/// Message type of a control request (C3, 5.0.0+ name `Invoke2ManagerMethod`).
pub const TYPE_CONTROL: u16 = 7;

/// Signature of a request header: `SFCI` (C6).
pub const IN_MAGIC: u32 = u32::from_le_bytes(*b"SFCI");
/// Signature of a reply header: `SFCO` (C6).
pub const OUT_MAGIC: u32 = u32::from_le_bytes(*b"SFCO");

/// Size of the CMIF header.
pub const HEADER_SIZE: usize = 0x10;
/// Total padding in the raw data (C5).
const PADDING: usize = 0x10;

/// A request or reply header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// [`IN_MAGIC`] or [`OUT_MAGIC`].
    pub magic: u32,
    /// 0, or 1 from firmware 5.0.0.
    pub version: u16,
    /// The method id of a request, or the result of a reply (`+8`).
    pub value: u32,
    /// The token of a request, or the interface id of a reply (`+0xC`).
    pub extra: u32,
}

/// Writes a HIPC message made of `parts` whose raw data is `header` and `params`; the raw
/// data in `parts` is ignored. Returns the bytes the message uses.
pub fn write(
    buf: &mut MessageBuffer,
    parts: &Parts<'_>,
    header: &Header,
    params: &[u8],
) -> Result<usize> {
    let mut parts = *parts;
    parts.raw = &[];
    let pre = padding_before(hipc::raw_offset(&parts));
    let len = PADDING + HEADER_SIZE + params.len().next_multiple_of(4);
    if len > MESSAGE_SIZE {
        return Err(IpcError::TooLarge { needed: len });
    }
    let mut raw = [0; MESSAGE_SIZE];
    let at = pre;
    raw[at..at + 4].copy_from_slice(&header.magic.to_le_bytes());
    raw[at + 4..at + 6].copy_from_slice(&header.version.to_le_bytes());
    raw[at + 8..at + 12].copy_from_slice(&header.value.to_le_bytes());
    raw[at + 12..at + 16].copy_from_slice(&header.extra.to_le_bytes());
    raw[at + HEADER_SIZE..at + HEADER_SIZE + params.len()].copy_from_slice(params);
    parts.raw = &raw[..len];
    hipc::write(buf, &parts)
}

/// Reads the CMIF header of `msg` and checks its signature is `magic`. Returns the header and
/// the bytes after it, which hold the parameters followed by the rest of the padding.
pub fn read<'a>(msg: &Message<'a>, magic: u32) -> Result<(Header, &'a [u8])> {
    let raw = msg.raw();
    let at = padding_before(msg.raw_offset());
    let bad = IpcError::BadCmifHeader { expected: magic };
    let header = raw.get(at..at + HEADER_SIZE).ok_or(bad)?;
    let word =
        |i: usize| u32::from_le_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]]);
    let header = Header {
        magic: word(0),
        version: u16::from_le_bytes([header[4], header[5]]),
        value: word(8),
        extra: word(12),
    };
    if header.magic != magic {
        return Err(bad);
    }
    Ok((header, &raw[at + HEADER_SIZE..]))
}

/// Padding from the start of the raw data to the next 16-byte boundary of the buffer; the
/// buffer itself is 16-byte aligned in TLS.
fn padding_before(raw_offset: usize) -> usize {
    raw_offset.next_multiple_of(16) - raw_offset
}

#[cfg(test)]
mod tests {
    use super::{Header, IN_MAGIC, OUT_MAGIC, TYPE_REQUEST, read, write};
    use crate::IpcError;
    use crate::hipc::{MESSAGE_SIZE, Message, Parts};

    #[test]
    fn a_request_round_trips_with_its_parameters() {
        let header = Header {
            magic: IN_MAGIC,
            version: 1,
            value: 17,
            extra: 0xAABB,
        };
        let parts = Parts {
            tag: TYPE_REQUEST,
            pid: Some(5),
            ..Parts::default()
        };
        let mut buf = [0; MESSAGE_SIZE];
        write(
            &mut buf,
            &parts,
            &header,
            &0x0102_0304_0506_0708u64.to_le_bytes(),
        )
        .unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(msg.tag(), TYPE_REQUEST);
        assert_eq!(msg.pid(), Some(5));
        // Header 0x14 bytes (two words, special header, PID): 12 bytes of padding first.
        assert_eq!(msg.raw_offset(), 0x14);
        assert_eq!(msg.raw().len(), 0x10 + 0x10 + 8);
        assert_eq!(&buf[0x20..0x24], b"SFCI", "the header is 16-byte aligned");
        let (read_header, params) = read(&msg, IN_MAGIC).unwrap();
        assert_eq!(read_header, header);
        assert_eq!(&params[..8], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert!(
            params[8..].iter().all(|&b| b == 0),
            "then the rest of the padding"
        );
    }

    #[test]
    fn a_reply_round_trips_with_a_moved_handle() {
        let header = Header {
            magic: OUT_MAGIC,
            value: 0xE401,
            ..Header::default()
        };
        let parts = Parts {
            move_handles: &[0x8003],
            ..Parts::default()
        };
        let mut buf = [0; MESSAGE_SIZE];
        write(&mut buf, &parts, &header, &[]).unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(msg.move_handles().collect::<Vec<_>>(), [0x8003]);
        assert_eq!(read(&msg, OUT_MAGIC).unwrap().0, header);
        assert_eq!(
            read(&msg, IN_MAGIC),
            Err(IpcError::BadCmifHeader { expected: IN_MAGIC })
        );
    }

    #[test]
    fn raw_data_too_short_for_a_header_is_rejected() {
        let mut buf = [0; MESSAGE_SIZE];
        crate::hipc::write(
            &mut buf,
            &Parts {
                raw: &[0; 8],
                ..Parts::default()
            },
        )
        .unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert!(read(&msg, IN_MAGIC).is_err());
    }
}
