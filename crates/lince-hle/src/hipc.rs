// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The HIPC message: the outer layout of every IPC request and reply.
//!
//! In order: two header words, an optional special header (PID, copy handles, move handles),
//! pointer descriptors, send, receive and exchange buffer descriptors, the raw data and the
//! receive list. Bit positions: docs/research/b2-svcs-and-ipc.md C1, C2, C4 (switchbrew HIPC
//! page, revision 14205).

use crate::error::{IpcError, Result};

/// Size of the message buffer at the start of a thread's TLS block.
pub const MESSAGE_SIZE: usize = 0x100;

/// A thread's IPC message buffer.
pub type MessageBuffer = [u8; MESSAGE_SIZE];

/// Highest guest address a descriptor can hold: addresses are 39 bits.
const ADDRESS_LIMIT: u64 = 1 << 39;
/// Highest size a send, receive or exchange descriptor can hold: sizes are 36 bits.
const MAP_SIZE_LIMIT: u64 = 1 << 36;
/// Offset of the PID after the special header (C2).
const PID_AT: usize = 0xC;
/// Most entries of a 4-bit count field.
const COUNT_MAX: usize = 0xF;
/// Most raw-data words: the field is 10 bits.
const RAW_WORDS_MAX: usize = 0x3FF;
/// The receive-list count that means "one inline buffer, no entry" (C1).
const RECEIVE_LIST_INLINE: u32 = 1;
/// Receive-list counts from this value up mean `count - 2` entries (C1).
const RECEIVE_LIST_FIRST_ENTRY: u32 = 2;

/// A pointer (X) descriptor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pointer {
    /// Which receive-list entry the data is copied to (4 bits).
    pub index: u8,
    /// Guest address (39 bits).
    pub address: u64,
    /// Size in bytes.
    pub size: u16,
}

/// A send (A), receive (B) or exchange (W) buffer descriptor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Map {
    /// Guest address (39 bits).
    pub address: u64,
    /// Size in bytes (36 bits).
    pub size: u64,
    /// Transfer attribute: bit 0 allows non-secure, bit 1 allows non-device memory.
    pub attribute: u8,
}

/// A receive-list (C) entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReceiveEntry {
    /// Guest address (39 bits).
    pub address: u64,
    /// Size in bytes.
    pub size: u16,
}

/// The receive list of a message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReceiveList<'s> {
    /// No receive list.
    #[default]
    None,
    /// Pointer data is copied into the message buffer after the raw data; no entry.
    Inline,
    /// These entries, at least one.
    Entries(&'s [ReceiveEntry]),
}

/// The parts of a message to write with [`write`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Parts<'s> {
    /// The message type (low 16 bits of the first word).
    pub tag: u16,
    /// The process id to send, if any.
    pub pid: Option<u64>,
    /// Handles copied to the receiver.
    pub copy_handles: &'s [u32],
    /// Handles moved to the receiver.
    pub move_handles: &'s [u32],
    /// Pointer descriptors.
    pub pointers: &'s [Pointer],
    /// Send buffer descriptors.
    pub sends: &'s [Map],
    /// Receive buffer descriptors.
    pub receives: &'s [Map],
    /// Exchange buffer descriptors.
    pub exchanges: &'s [Map],
    /// Raw data, a whole number of 32-bit words.
    pub raw: &'s [u8],
    /// The receive list.
    pub receive_list: ReceiveList<'s>,
}

/// Where each part of a message starts; derived from the counts in its header.
#[derive(Clone, Copy, Debug, Default)]
struct Layout {
    special: bool,
    pid: bool,
    copies: usize,
    moves: usize,
    pointers: usize,
    sends: usize,
    receives: usize,
    exchanges: usize,
    raw_words: usize,
    receive_entries: usize,
}

impl Layout {
    fn copies_at(&self) -> usize {
        PID_AT + if self.pid { 8 } else { 0 }
    }

    fn moves_at(&self) -> usize {
        self.copies_at() + 4 * self.copies
    }

    fn pointers_at(&self) -> usize {
        if self.special {
            self.moves_at() + 4 * self.moves
        } else {
            8
        }
    }

    fn sends_at(&self) -> usize {
        self.pointers_at() + 8 * self.pointers
    }

    fn receives_at(&self) -> usize {
        self.sends_at() + 12 * self.sends
    }

    fn exchanges_at(&self) -> usize {
        self.receives_at() + 12 * self.receives
    }

    fn raw_at(&self) -> usize {
        self.exchanges_at() + 12 * self.exchanges
    }

    // GUESS: the list follows the raw data. The page names a ReceiveListOffset field (word 1
    // bits 20-30) without its unit; it is written as 0 and ignored when read.
    fn receive_list_at(&self) -> usize {
        self.raw_at() + 4 * self.raw_words
    }

    fn end(&self) -> usize {
        self.receive_list_at() + 8 * self.receive_entries
    }

    fn of(parts: &Parts<'_>) -> Self {
        let special =
            parts.pid.is_some() || !parts.copy_handles.is_empty() || !parts.move_handles.is_empty();
        Self {
            special,
            pid: parts.pid.is_some(),
            copies: parts.copy_handles.len(),
            moves: parts.move_handles.len(),
            pointers: parts.pointers.len(),
            sends: parts.sends.len(),
            receives: parts.receives.len(),
            exchanges: parts.exchanges.len(),
            raw_words: parts.raw.len() / 4,
            receive_entries: match parts.receive_list {
                ReceiveList::Entries(entries) => entries.len(),
                ReceiveList::None | ReceiveList::Inline => 0,
            },
        }
    }
}

/// Offset of the raw data in the buffer once `parts` is written.
pub fn raw_offset(parts: &Parts<'_>) -> usize {
    Layout::of(parts).raw_at()
}

/// Writes `parts` to `buf`, zeroing the rest of it; returns the bytes the message uses.
pub fn write(buf: &mut MessageBuffer, parts: &Parts<'_>) -> Result<usize> {
    check(parts)?;
    let layout = Layout::of(parts);
    let end = layout.end();
    if end > MESSAGE_SIZE {
        return Err(IpcError::TooLarge { needed: end });
    }
    let receive_list = match parts.receive_list {
        ReceiveList::None => 0,
        ReceiveList::Inline => RECEIVE_LIST_INLINE,
        ReceiveList::Entries(entries) => count(entries.len())? + RECEIVE_LIST_FIRST_ENTRY,
    };
    buf.fill(0);
    put(
        buf,
        0,
        u32::from(parts.tag)
            | count(layout.pointers)? << 16
            | count(layout.sends)? << 20
            | count(layout.receives)? << 24
            | count(layout.exchanges)? << 28,
    );
    put(
        buf,
        4,
        to_u32(layout.raw_words) | receive_list << 10 | u32::from(layout.special) << 31,
    );
    if layout.special {
        put(
            buf,
            8,
            u32::from(layout.pid) | count(layout.copies)? << 1 | count(layout.moves)? << 5,
        );
    }
    if let Some(pid) = parts.pid {
        buf[PID_AT..PID_AT + 8].copy_from_slice(&pid.to_le_bytes());
    }
    put_words(buf, layout.copies_at(), parts.copy_handles);
    put_words(buf, layout.moves_at(), parts.move_handles);
    for (i, pointer) in parts.pointers.iter().enumerate() {
        put_pointer(buf, layout.pointers_at() + 8 * i, pointer);
    }
    put_maps(buf, layout.sends_at(), parts.sends);
    put_maps(buf, layout.receives_at(), parts.receives);
    put_maps(buf, layout.exchanges_at(), parts.exchanges);
    buf[layout.raw_at()..layout.receive_list_at()].copy_from_slice(parts.raw);
    if let ReceiveList::Entries(entries) = parts.receive_list {
        for (i, entry) in entries.iter().enumerate() {
            let at = layout.receive_list_at() + 8 * i;
            put(buf, at, low(entry.address));
            put(
                buf,
                at + 4,
                high(entry.address) & 0x7F | u32::from(entry.size) << 16,
            );
        }
    }
    Ok(end)
}

/// Rejects values that do not fit their fields.
fn check(parts: &Parts<'_>) -> Result<()> {
    let overflow = |field| Err(IpcError::FieldOverflow { field });
    if !parts.raw.len().is_multiple_of(4) {
        return Err(IpcError::UnalignedRaw {
            len: parts.raw.len(),
        });
    }
    if parts.raw.len() / 4 > RAW_WORDS_MAX {
        return overflow("raw size");
    }
    if parts
        .pointers
        .iter()
        .any(|p| usize::from(p.index) > COUNT_MAX || p.address >= ADDRESS_LIMIT)
    {
        return overflow("pointer descriptor");
    }
    let bad_map =
        |m: &Map| m.address >= ADDRESS_LIMIT || m.size >= MAP_SIZE_LIMIT || m.attribute > 3;
    if [parts.sends, parts.receives, parts.exchanges]
        .iter()
        .any(|maps| maps.iter().any(bad_map))
    {
        return overflow("buffer descriptor");
    }
    match parts.receive_list {
        ReceiveList::Entries([]) => overflow("receive list"),
        ReceiveList::Entries(entries) if entries.iter().any(|e| e.address >= ADDRESS_LIMIT) => {
            overflow("receive list")
        }
        ReceiveList::None | ReceiveList::Inline | ReceiveList::Entries(_) => Ok(()),
    }
}

/// A message read from a buffer. Accessors read the buffer; nothing is copied.
#[derive(Clone, Copy, Debug)]
pub struct Message<'a> {
    buf: &'a MessageBuffer,
    layout: Layout,
    receive_list: u32,
}

impl<'a> Message<'a> {
    /// Reads the header of the message in `buf`; fails if its parts run past the buffer.
    pub fn parse(buf: &'a MessageBuffer) -> Result<Self> {
        let (word0, word1) = (get(buf, 0), get(buf, 4));
        let special = word1 >> 31 != 0;
        let special_word = if special { get(buf, 8) } else { 0 };
        let receive_list = word1 >> 10 & 0xF;
        let layout = Layout {
            special,
            pid: special_word & 1 != 0,
            copies: field(special_word, 1),
            moves: field(special_word, 5),
            pointers: field(word0, 16),
            sends: field(word0, 20),
            receives: field(word0, 24),
            exchanges: field(word0, 28),
            raw_words: to_usize(word1 & 0x3FF),
            receive_entries: to_usize(receive_list.saturating_sub(RECEIVE_LIST_FIRST_ENTRY)),
        };
        if layout.end() > MESSAGE_SIZE {
            return Err(IpcError::TooLarge {
                needed: layout.end(),
            });
        }
        Ok(Self {
            buf,
            layout,
            receive_list,
        })
    }

    /// The message type.
    pub fn tag(&self) -> u16 {
        u16::try_from(get(self.buf, 0) & 0xFFFF).unwrap_or(u16::MAX)
    }

    /// The process id sent with the message, if any.
    pub fn pid(&self) -> Option<u64> {
        let at = PID_AT;
        self.layout.pid.then(|| {
            let mut bytes = [0; 8];
            bytes.copy_from_slice(&self.buf[at..at + 8]);
            u64::from_le_bytes(bytes)
        })
    }

    /// The copied handles; 0 means no handle.
    pub fn copy_handles(&self) -> impl ExactSizeIterator<Item = u32> + 'a {
        words(self.buf, self.layout.copies_at(), self.layout.copies)
    }

    /// The moved handles; 0 means no handle.
    pub fn move_handles(&self) -> impl ExactSizeIterator<Item = u32> + 'a {
        words(self.buf, self.layout.moves_at(), self.layout.moves)
    }

    /// The pointer descriptors.
    pub fn pointers(&self) -> impl ExactSizeIterator<Item = Pointer> + 'a {
        let (buf, at) = (self.buf, self.layout.pointers_at());
        (0..self.layout.pointers).map(move |i| get_pointer(buf, at + 8 * i))
    }

    /// The send buffer descriptors.
    pub fn sends(&self) -> impl ExactSizeIterator<Item = Map> + 'a {
        maps(self.buf, self.layout.sends_at(), self.layout.sends)
    }

    /// The receive buffer descriptors.
    pub fn receives(&self) -> impl ExactSizeIterator<Item = Map> + 'a {
        maps(self.buf, self.layout.receives_at(), self.layout.receives)
    }

    /// The exchange buffer descriptors.
    pub fn exchanges(&self) -> impl ExactSizeIterator<Item = Map> + 'a {
        maps(self.buf, self.layout.exchanges_at(), self.layout.exchanges)
    }

    /// Offset of the raw data in the buffer.
    pub fn raw_offset(&self) -> usize {
        self.layout.raw_at()
    }

    /// The raw data.
    pub fn raw(&self) -> &'a [u8] {
        &self.buf[self.layout.raw_at()..self.layout.receive_list_at()]
    }

    /// Whether pointer data goes inline after the raw data instead of to receive-list entries.
    pub fn receive_list_inline(&self) -> bool {
        self.receive_list == RECEIVE_LIST_INLINE
    }

    /// The receive-list entries.
    pub fn receive_entries(&self) -> impl ExactSizeIterator<Item = ReceiveEntry> + 'a {
        let (buf, at) = (self.buf, self.layout.receive_list_at());
        (0..self.layout.receive_entries).map(move |i| {
            let word1 = get(buf, at + 8 * i + 4);
            ReceiveEntry {
                address: u64::from(get(buf, at + 8 * i)) | u64::from(word1 & 0x7F) << 32,
                size: half(word1 >> 16),
            }
        })
    }
}

fn get(buf: &MessageBuffer, at: usize) -> u32 {
    let mut bytes = [0; 4];
    bytes.copy_from_slice(&buf[at..at + 4]);
    u32::from_le_bytes(bytes)
}

fn put(buf: &mut MessageBuffer, at: usize, word: u32) {
    buf[at..at + 4].copy_from_slice(&word.to_le_bytes());
}

fn put_words(buf: &mut MessageBuffer, at: usize, words: &[u32]) {
    for (i, &word) in words.iter().enumerate() {
        put(buf, at + 4 * i, word);
    }
}

fn words(buf: &MessageBuffer, at: usize, n: usize) -> impl ExactSizeIterator<Item = u32> + '_ {
    (0..n).map(move |i| get(buf, at + 4 * i))
}

fn put_pointer(buf: &mut MessageBuffer, at: usize, pointer: &Pointer) {
    let high = high(pointer.address);
    put(
        buf,
        at,
        u32::from(pointer.index)
            | (high >> 4 & 0x7) << 6
            | (high & 0xF) << 12
            | u32::from(pointer.size) << 16,
    );
    put(buf, at + 4, low(pointer.address));
}

fn get_pointer(buf: &MessageBuffer, at: usize) -> Pointer {
    let word0 = get(buf, at);
    Pointer {
        index: byte(word0 & 0xF),
        address: u64::from(get(buf, at + 4))
            | u64::from(word0 >> 12 & 0xF) << 32
            | u64::from(word0 >> 6 & 0x7) << 36,
        size: half(word0 >> 16),
    }
}

fn put_maps(buf: &mut MessageBuffer, at: usize, maps: &[Map]) {
    for (i, map) in maps.iter().enumerate() {
        let (at, high) = (at + 12 * i, high(map.address));
        put(buf, at, low(map.size));
        put(buf, at + 4, low(map.address));
        put(
            buf,
            at + 8,
            u32::from(map.attribute)
                | (high >> 4 & 0x7) << 2
                | (self::high(map.size) & 0xF) << 24
                | (high & 0xF) << 28,
        );
    }
}

fn maps(buf: &MessageBuffer, at: usize, n: usize) -> impl ExactSizeIterator<Item = Map> + '_ {
    (0..n).map(move |i| {
        let at = at + 12 * i;
        let word2 = get(buf, at + 8);
        Map {
            address: u64::from(get(buf, at + 4))
                | u64::from(word2 >> 28) << 32
                | u64::from(word2 >> 2 & 0x7) << 36,
            size: u64::from(get(buf, at)) | u64::from(word2 >> 24 & 0xF) << 32,
            attribute: byte(word2 & 0x3),
        }
    })
}

/// A 4-bit count field at `shift`.
fn field(word: u32, shift: u32) -> usize {
    to_usize(word >> shift & 0xF)
}

fn count(n: usize) -> Result<u32> {
    if n > COUNT_MAX {
        return Err(IpcError::FieldOverflow { field: "count" });
    }
    Ok(to_u32(n))
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn to_usize(n: u32) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

fn low(value: u64) -> u32 {
    u32::try_from(value & 0xFFFF_FFFF).unwrap_or(u32::MAX)
}

fn high(value: u64) -> u32 {
    low(value >> 32)
}

fn half(value: u32) -> u16 {
    u16::try_from(value & 0xFFFF).unwrap_or(u16::MAX)
}

fn byte(value: u32) -> u8 {
    u8::try_from(value & 0xFF).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::{
        MESSAGE_SIZE, Map, Message, MessageBuffer, Parts, Pointer, ReceiveEntry, ReceiveList,
        raw_offset, write,
    };
    use crate::IpcError;

    fn words(buf: &MessageBuffer, n: usize) -> Vec<u32> {
        buf.as_chunks::<4>()
            .0
            .iter()
            .take(n)
            .map(|c| u32::from_le_bytes(*c))
            .collect()
    }

    #[test]
    fn every_field_round_trips() {
        let pointers = [Pointer {
            index: 2,
            address: 0x7F_1234_5678,
            size: 0x40,
        }];
        let maps = [
            Map {
                address: 0x55_0000_1000,
                size: 0x3_0000_0200,
                attribute: 1,
            },
            Map {
                address: 0x1000,
                size: 0x10,
                attribute: 2,
            },
        ];
        let entries = [ReceiveEntry {
            address: 0x40_0000_8000,
            size: 0x80,
        }];
        let raw = [0xAB; 16];
        let parts = Parts {
            tag: 6,
            pid: Some(0x1122_3344_5566_7788),
            copy_handles: &[0x8001, 0],
            move_handles: &[0x8002],
            pointers: &pointers,
            sends: &maps[..1],
            receives: &maps[1..],
            exchanges: &maps,
            raw: &raw,
            receive_list: ReceiveList::Entries(&entries),
        };
        let mut buf = [0xFF; MESSAGE_SIZE];
        let used = write(&mut buf, &parts).unwrap();
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(msg.tag(), 6);
        assert_eq!(msg.pid(), parts.pid);
        assert_eq!(msg.copy_handles().collect::<Vec<_>>(), parts.copy_handles);
        assert_eq!(msg.move_handles().collect::<Vec<_>>(), parts.move_handles);
        assert_eq!(msg.pointers().collect::<Vec<_>>(), pointers);
        assert_eq!(msg.sends().collect::<Vec<_>>(), &maps[..1]);
        assert_eq!(msg.receives().collect::<Vec<_>>(), &maps[1..]);
        assert_eq!(msg.exchanges().collect::<Vec<_>>(), maps);
        assert_eq!(msg.raw(), raw);
        assert_eq!(msg.raw_offset(), raw_offset(&parts));
        assert_eq!(msg.receive_entries().collect::<Vec<_>>(), entries);
        assert!(!msg.receive_list_inline());
        assert!(buf[used..].iter().all(|&b| b == 0), "the rest is zeroed");
    }

    #[test]
    fn a_message_without_handles_has_no_special_header() {
        let parts = Parts {
            tag: 4,
            raw: &[1, 2, 3, 4],
            receive_list: ReceiveList::Inline,
            ..Parts::default()
        };
        let mut buf = [0; MESSAGE_SIZE];
        assert_eq!(write(&mut buf, &parts).unwrap(), 12);
        assert_eq!(words(&buf, 3), [4, 1 | 1 << 10, 0x0403_0201]);
        let msg = Message::parse(&buf).unwrap();
        assert_eq!(msg.pid(), None);
        assert_eq!(msg.raw_offset(), 8);
        assert!(msg.receive_list_inline());
        assert_eq!(msg.receive_entries().len(), 0);
    }

    /// Bytes laid out by hand from the bit tables in the research note (C1, C2, C4), so a
    /// mistake shared by `write` and `parse` still fails.
    #[test]
    fn bit_positions_match_the_documented_layout() {
        let pointers = [Pointer {
            index: 2,
            address: 0x7F_1234_5678,
            size: 0x40,
        }];
        let sends = [Map {
            address: 0x55_0000_1000,
            size: 0x3_0000_0200,
            attribute: 1,
        }];
        let parts = Parts {
            tag: 4,
            pid: Some(0x99),
            copy_handles: &[0x8001],
            pointers: &pointers,
            sends: &sends,
            raw: &[0; 8],
            receive_list: ReceiveList::Entries(&[ReceiveEntry {
                address: 0x40_0000_8000,
                size: 0x80,
            }]),
            ..Parts::default()
        };
        let mut buf = [0; MESSAGE_SIZE];
        write(&mut buf, &parts).unwrap();
        let expected = [
            4 | 1 << 16 | 1 << 20,               // tag, one pointer, one send
            2 | 3 << 10 | 1 << 31,               // two raw words, one receive entry, special header
            1 | 1 << 1,                          // PID, one copy handle
            0x99,                                // PID low (0xC)
            0,                                   // PID high
            0x8001,                              // copy handle (0x14)
            2 | 7 << 6 | 0xF << 12 | 0x40 << 16, // pointer: index, addr 36-38, 32-35, size
            0x1234_5678,                         // pointer address low
            0x0000_0200,                         // send: size low
            0x0000_1000,                         // send: address low
            1 | 5 << 2 | 3 << 24 | 5 << 28,      // attribute, addr 36-38, size 32-35, addr 32-35
            0,                                   // raw
            0,                                   // raw
            0x8000,                              // receive entry: address low
            0x40 | 0x80 << 16,                   // address 32-38, size
        ];
        assert_eq!(words(&buf, expected.len()), expected);
    }

    #[test]
    fn oversized_and_out_of_range_parts_are_rejected() {
        let mut buf = [0; MESSAGE_SIZE];
        let big = [0; MESSAGE_SIZE];
        let parts = Parts {
            raw: &big,
            ..Parts::default()
        };
        assert_eq!(
            write(&mut buf, &parts),
            Err(IpcError::TooLarge {
                needed: 8 + MESSAGE_SIZE
            })
        );
        let parts = Parts {
            raw: &[0; 3],
            ..Parts::default()
        };
        assert_eq!(
            write(&mut buf, &parts),
            Err(IpcError::UnalignedRaw { len: 3 })
        );
        let far = [Pointer {
            address: 1 << 39,
            ..Pointer::default()
        }];
        let parts = Parts {
            pointers: &far,
            ..Parts::default()
        };
        assert!(matches!(
            write(&mut buf, &parts),
            Err(IpcError::FieldOverflow { .. })
        ));
        let parts = Parts {
            receive_list: ReceiveList::Entries(&[]),
            ..Parts::default()
        };
        assert!(matches!(
            write(&mut buf, &parts),
            Err(IpcError::FieldOverflow { .. })
        ));
    }

    #[test]
    fn a_header_whose_counts_run_past_the_buffer_is_rejected() {
        let mut buf = [0; MESSAGE_SIZE];
        buf[4..8].copy_from_slice(&0x3FFu32.to_le_bytes());
        assert!(matches!(
            Message::parse(&buf),
            Err(IpcError::TooLarge { .. })
        ));
    }
}
