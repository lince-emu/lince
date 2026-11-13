// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ledger events (docs/05-performance-validation.md §3) and their 24-byte encoding.
//!
//! ```text
//! w0  timestamp, nanoseconds since the ledger epoch
//! w1  frame u32 | kind u16 << 32 | reserved u16 << 48
//! w2  argument; its meaning depends on the kind (see `EventKind::blocking_ns`)
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
            other => Self::Unknown(other),
        }
    }

    /// Time the recording thread spent blocked or busy because of this event, for kinds whose
    /// argument is a duration; `None` for the others.
    pub fn blocking_ns(self, arg: u64) -> Option<u64> {
        match self {
            Self::CompileEnd
            | Self::PipelineWait
            | Self::CacheResolve
            | Self::CacheWrite
            | Self::CacheFsync
            | Self::FenceWait => Some(arg),
            Self::FrameBegin
            | Self::FrameEnd
            | Self::PresentQueued
            | Self::Presented
            | Self::GuestFrameFence
            | Self::ShaderDiscovered
            | Self::Tier0Used
            | Self::CompileStart
            | Self::Alloc
            | Self::Upload
            | Self::Evict
            | Self::QueueSubmit
            | Self::MacroBurst
            | Self::HleCall
            | Self::GovernorThrottle
            // Preemption is OS or worker interference, a separate class in doc 05 §4, so it
            // is not counted as emulator blocking time.
            | Self::ThreadPreempted
            | Self::Unknown(_) => None,
        }
    }
}

/// The role of the thread that owns a ring. Raw values are part of the dump format.
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

    /// Render-critical threads (AGENTS.md invariant 2): time they lose delays the frame.
    pub fn is_render_critical(self) -> bool {
        matches!(
            self,
            Self::GuestCore | Self::GpuFront | Self::GpuBack | Self::Submit | Self::Present
        )
    }
}

/// One ledger event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Nanoseconds since the ledger epoch; for duration kinds, the end of the interval.
    pub ts_ns: u64,
    /// Frame sequence number.
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

    const KINDS: [EventKind; 22] = [
        EventKind::FrameBegin,
        EventKind::FrameEnd,
        EventKind::PresentQueued,
        EventKind::Presented,
        EventKind::GuestFrameFence,
        EventKind::ShaderDiscovered,
        EventKind::Tier0Used,
        EventKind::CompileStart,
        EventKind::CompileEnd,
        EventKind::PipelineWait,
        EventKind::CacheResolve,
        EventKind::CacheWrite,
        EventKind::CacheFsync,
        EventKind::Alloc,
        EventKind::Upload,
        EventKind::Evict,
        EventKind::FenceWait,
        EventKind::QueueSubmit,
        EventKind::MacroBurst,
        EventKind::HleCall,
        EventKind::GovernorThrottle,
        EventKind::ThreadPreempted,
    ];

    const ROLES: [ThreadRole; 8] = [
        ThreadRole::GuestCore,
        ThreadRole::GpuFront,
        ThreadRole::GpuBack,
        ThreadRole::Submit,
        ThreadRole::Present,
        ThreadRole::CompileWorker,
        ThreadRole::CacheIo,
        ThreadRole::Other,
    ];

    #[test]
    fn every_kind_and_role_round_trips() {
        for kind in KINDS {
            assert_eq!(EventKind::from_raw(kind.to_raw()), kind);
        }
        for role in ROLES {
            assert_eq!(ThreadRole::from_raw(role.to_raw()), role);
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
            kind: EventKind::PipelineWait,
            arg: 7_000_000,
        };
        assert_eq!(Event::decode(e.encode()), e);
    }
}
