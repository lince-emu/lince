// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ledger events (docs/05-performance-validation.md §3) and their 24-byte encoding.
//!
//! This is the event shape prototyped in `poc/frame-ledger` (PoC-7), so the ledger promoted
//! later reads the records written here (ADR-0027, B1 scope).
//!
//! ```text
//! w0  timestamp, nanoseconds since the trace epoch
//! w1  frame u32 | kind u16 << 32 | reserved u16 << 48
//! w2  argument; its meaning depends on the kind
//! ```

/// What happened. Raw values are part of the dump format; unknown values are kept (PAT-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// A frame started. `arg`: unused.
    FrameBegin,
    /// A frame ended. `arg`: unused.
    FrameEnd,
    /// A frame was queued for presentation.
    PresentQueued,
    /// A frame was presented.
    Presented,
    /// The guest signalled its frame fence.
    GuestFrameFence,
    /// A new guest shader program was seen. `arg`: low bits of its id.
    ShaderDiscovered,
    /// A draw used the Tier-0 interpreter.
    Tier0Used,
    /// A compile started. `arg`: tier.
    CompileStart,
    /// A compile ended. `arg`: nanoseconds spent compiling on the recording thread.
    CompileEnd,
    /// A draw waited for a pipeline. `arg`: nanoseconds waited.
    PipelineWait,
    /// A cache lookup ran. `arg`: nanoseconds spent.
    CacheResolve,
    /// A cache write ran. `arg`: nanoseconds spent.
    CacheWrite,
    /// A cache fsync ran. `arg`: nanoseconds spent.
    CacheFsync,
    /// Memory was allocated. `arg`: bytes.
    Alloc,
    /// Data was uploaded to the GPU. `arg`: bytes.
    Upload,
    /// A resource was evicted. `arg`: bytes.
    Evict,
    /// The thread waited on a fence. `arg`: nanoseconds waited.
    FenceWait,
    /// Work was submitted to a queue.
    QueueSubmit,
    /// A burst of guest macro calls ran. `arg`: call count.
    MacroBurst,
    /// An HLE service call ran. `arg`: service id.
    HleCall,
    /// The governor throttled background work. `arg`: level.
    GovernorThrottle,
    /// The thread was preempted (sampled). `arg`: nanoseconds lost.
    ThreadPreempted,
    /// A named stub ran. `arg`: its [`NameId`](crate::NameId).
    Stub,
    /// The guest called an SVC with no handler. `arg`: the SVC number.
    UnknownSvc,
    /// A kind this build does not know.
    Unknown(u16),
}

impl EventKind {
    /// Raw value used in the encoding (a constant-time `match` on the recording path).
    pub fn to_raw(self) -> u16 {
        match self {
            Self::FrameBegin => 0,
            Self::FrameEnd => 1,
            Self::PresentQueued => 2,
            Self::Presented => 3,
            Self::GuestFrameFence => 4,
            Self::ShaderDiscovered => 5,
            Self::Tier0Used => 6,
            Self::CompileStart => 7,
            Self::CompileEnd => 8,
            Self::PipelineWait => 9,
            Self::CacheResolve => 10,
            Self::CacheWrite => 11,
            Self::CacheFsync => 12,
            Self::Alloc => 13,
            Self::Upload => 14,
            Self::Evict => 15,
            Self::FenceWait => 16,
            Self::QueueSubmit => 17,
            Self::MacroBurst => 18,
            Self::HleCall => 19,
            Self::GovernorThrottle => 20,
            Self::ThreadPreempted => 21,
            Self::Stub => 22,
            Self::UnknownSvc => 23,
            Self::Unknown(raw) => raw,
        }
    }

    /// Decodes a raw value; unknown values are preserved.
    pub fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::FrameBegin,
            1 => Self::FrameEnd,
            2 => Self::PresentQueued,
            3 => Self::Presented,
            4 => Self::GuestFrameFence,
            5 => Self::ShaderDiscovered,
            6 => Self::Tier0Used,
            7 => Self::CompileStart,
            8 => Self::CompileEnd,
            9 => Self::PipelineWait,
            10 => Self::CacheResolve,
            11 => Self::CacheWrite,
            12 => Self::CacheFsync,
            13 => Self::Alloc,
            14 => Self::Upload,
            15 => Self::Evict,
            16 => Self::FenceWait,
            17 => Self::QueueSubmit,
            18 => Self::MacroBurst,
            19 => Self::HleCall,
            20 => Self::GovernorThrottle,
            21 => Self::ThreadPreempted,
            22 => Self::Stub,
            23 => Self::UnknownSvc,
            other => Self::Unknown(other),
        }
    }
}

/// The role of the thread that owns a buffer. Raw values are part of the dump format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadRole {
    /// Emulated guest CPU core.
    GuestCore,
    /// GPU command-processing front end.
    GpuFront,
    /// GPU back end (host command recording).
    GpuBack,
    /// Vulkan queue submission.
    Submit,
    /// Presentation.
    Present,
    /// Background compile worker.
    CompileWorker,
    /// Cache I/O thread.
    CacheIo,
    /// Anything else.
    Other,
    /// A role this build does not know.
    Unknown(u8),
}

impl ThreadRole {
    /// Raw value used in the dump format.
    pub fn to_raw(self) -> u8 {
        match self {
            Self::GuestCore => 0,
            Self::GpuFront => 1,
            Self::GpuBack => 2,
            Self::Submit => 3,
            Self::Present => 4,
            Self::CompileWorker => 5,
            Self::CacheIo => 6,
            Self::Other => 7,
            Self::Unknown(raw) => raw,
        }
    }

    /// Decodes a raw value; unknown values are preserved.
    pub fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::GuestCore,
            1 => Self::GpuFront,
            2 => Self::GpuBack,
            3 => Self::Submit,
            4 => Self::Present,
            5 => Self::CompileWorker,
            6 => Self::CacheIo,
            7 => Self::Other,
            other => Self::Unknown(other),
        }
    }
}

/// One ledger event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Nanoseconds since the trace epoch; for duration kinds, the end of the interval.
    pub ts_ns: u64,
    /// Frame sequence number (0 until frames exist).
    pub frame: u32,
    /// What happened.
    pub kind: EventKind,
    /// Kind-specific argument.
    pub arg: u64,
}

impl Event {
    /// Encodes the event as three words.
    pub fn encode(self) -> [u64; 3] {
        let w1 = u64::from(self.frame) | (u64::from(self.kind.to_raw()) << 32);
        [self.ts_ns, w1, self.arg]
    }

    /// Decodes three words.
    pub fn decode(words: [u64; 3]) -> Self {
        let w1 = words[1].to_le_bytes();
        Self {
            ts_ns: words[0],
            frame: u32::from_le_bytes([w1[0], w1[1], w1[2], w1[3]]),
            kind: EventKind::from_raw(u16::from_le_bytes([w1[4], w1[5]])),
            arg: words[2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, EventKind, ThreadRole};

    #[test]
    fn every_known_kind_and_role_round_trips() {
        for raw in 0..=23 {
            let kind = EventKind::from_raw(raw);
            assert!(!matches!(kind, EventKind::Unknown(_)), "raw {raw}");
            assert_eq!(kind.to_raw(), raw);
        }
        for raw in 0..=7 {
            let role = ThreadRole::from_raw(raw);
            assert!(!matches!(role, ThreadRole::Unknown(_)), "raw {raw}");
            assert_eq!(role.to_raw(), raw);
        }
    }

    #[test]
    fn unknown_values_are_preserved() {
        assert_eq!(EventKind::from_raw(999), EventKind::Unknown(999));
        assert_eq!(EventKind::Unknown(999).to_raw(), 999);
        assert_eq!(ThreadRole::from_raw(200).to_raw(), 200);
    }

    #[test]
    fn event_encoding_round_trips() {
        let e = Event {
            ts_ns: 123_456_789,
            frame: 42,
            kind: EventKind::UnknownSvc,
            arg: 0x80,
        };
        assert_eq!(Event::decode(e.encode()), e);
    }
}
