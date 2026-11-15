// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! TIPC: the CMIF-less messages `sm:` speaks from firmware 12.0.0 (docs/research/b2-svcs-and-
//! ipc.md T1–T3). The request id is the message type and the raw data is the payload.
//!
//! GUESS (research O1): the message type of a request is 16 plus the command id, and a reply
//! has message type 0 and carries the result in its first raw word, the payload after it. The
//! switchbrew page states neither; every value lives in the constants below so a corrected
//! layout changes one place.

use crate::error::{IpcError, Result};
use crate::hipc::{self, MESSAGE_SIZE, Message, MessageBuffer, Parts};

/// Added to a command id to give the request's message type (GUESS, T3).
pub const COMMAND_BASE: u16 = 16;
/// Message type of a reply (GUESS, O1).
pub const REPLY_TYPE: u16 = 0;

/// Writes a request for `command` with `payload`; the tag and raw data in `parts` are ignored.
pub fn write_request(
    buf: &mut MessageBuffer,
    parts: &Parts<'_>,
    command: u16,
    payload: &[u8],
) -> Result<usize> {
    let tag = COMMAND_BASE
        .checked_add(command)
        .ok_or(IpcError::FieldOverflow { field: "command" })?;
    write_raw(buf, parts, tag, &[], payload)
}

/// The command id of the request in `msg`.
pub fn command(msg: &Message<'_>) -> Result<u16> {
    let tag = msg.tag();
    tag.checked_sub(COMMAND_BASE)
        .ok_or(IpcError::NotTipcRequest { tag })
}

/// Writes a reply with `result` and `payload`; the tag and raw data in `parts` are ignored.
pub fn write_reply(
    buf: &mut MessageBuffer,
    parts: &Parts<'_>,
    result: u32,
    payload: &[u8],
) -> Result<usize> {
    write_raw(buf, parts, REPLY_TYPE, &result.to_le_bytes(), payload)
}

/// The result and payload of the reply in `msg`; the payload includes any padding to a word.
pub fn read_reply<'a>(msg: &Message<'a>) -> Result<(u32, &'a [u8])> {
    let raw = msg.raw();
    let result = raw
        .first_chunk::<4>()
        .ok_or(IpcError::TooShort { len: raw.len() })?;
    Ok((u32::from_le_bytes(*result), &raw[4..]))
}

fn write_raw(
    buf: &mut MessageBuffer,
    parts: &Parts<'_>,
    tag: u16,
    head: &[u8],
    payload: &[u8],
) -> Result<usize> {
    let len = (head.len() + payload.len()).next_multiple_of(4);
    if len > MESSAGE_SIZE {
        return Err(IpcError::TooLarge { needed: len });
    }
    let mut raw = [0; MESSAGE_SIZE];
    raw[..head.len()].copy_from_slice(head);
    raw[head.len()..head.len() + payload.len()].copy_from_slice(payload);
    let mut parts = *parts;
    parts.tag = tag;
    parts.raw = &raw[..len];
    hipc::write(buf, &parts)
}

#[cfg(test)]
mod tests {
    use super::{COMMAND_BASE, command, read_reply, write_reply, write_request};
    use crate::IpcError;
    use crate::hipc::{MESSAGE_SIZE, Message, Parts};

    #[test]
    fn a_request_round_trips_its_command_and_payload() {
        let name = u64::from_le_bytes(*b"set:\0\0\0\0").to_le_bytes();
        let mut buf = [0; MESSAGE_SIZE];
        write_request(&mut buf, &Parts::default(), 1, &name).unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(msg.tag(), COMMAND_BASE + 1);
        assert_eq!(command(&msg), Ok(1));
        assert_eq!(msg.raw(), name, "the raw data is the payload");
    }

    #[test]
    fn a_reply_round_trips_its_result_and_moved_handle() {
        let parts = Parts {
            move_handles: &[0x8005],
            ..Parts::default()
        };
        let mut buf = [0; MESSAGE_SIZE];
        write_reply(&mut buf, &parts, 0x415, &[7]).unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(msg.move_handles().collect::<Vec<_>>(), [0x8005]);
        let (result, payload) = read_reply(&msg).unwrap();
        assert_eq!(result, 0x415);
        assert_eq!(payload, [7, 0, 0, 0], "padded to a word");
    }

    #[test]
    fn a_reply_without_a_result_word_is_too_short() {
        let mut buf = [0; MESSAGE_SIZE];
        crate::hipc::write(&mut buf, &Parts::default()).unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(read_reply(&msg), Err(IpcError::TooShort { len: 0 }));
    }

    #[test]
    fn a_cmif_message_type_is_not_a_tipc_request() {
        let mut buf = [0; MESSAGE_SIZE];
        crate::hipc::write(
            &mut buf,
            &Parts {
                tag: 6,
                ..Parts::default()
            },
        )
        .unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(command(&msg), Err(IpcError::NotTipcRequest { tag: 6 }));
    }
}
