// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The ledger: a fixed set of per-thread rings and the recorders that write them.
//!
//! Threads: `Recorder::record` runs on the owning thread, possibly render-critical, and
//! never blocks or allocates. `Ledger::register` allocates the ring and `Ledger::dump`
//! allocates and copies, so both run off render-critical threads (ARCH-06).

use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use crate::dump::{Dump, ThreadDump};
use crate::event::{Event, EventKind, ThreadRole};
use crate::ring::Ring;

/// A registered thread's ring.
struct Registered {
    role: ThreadRole,
    ring: Arc<Ring>,
}

/// The ledger. Shared by reference between threads; registration and dumps are lock-free.
pub struct Ledger {
    threads: Box<[OnceLock<Registered>]>,
    registered: AtomicUsize,
    capacity_log2: u32,
    epoch: Instant,
}

impl Ledger {
    /// A ledger for up to `max_threads` threads, each keeping its last `2^capacity_log2` events.
    ///
    /// # Panics
    ///
    /// If `capacity_log2` is outside `1..=24` (2 to 16 Mi events per thread).
    pub fn new(max_threads: usize, capacity_log2: u32) -> Self {
        assert!(
            (1..=24).contains(&capacity_log2),
            "ring capacity 2^{capacity_log2} is outside 2^1..=2^24"
        );
        Self {
            threads: (0..max_threads).map(|_| OnceLock::new()).collect(),
            registered: AtomicUsize::new(0),
            capacity_log2,
            epoch: Instant::now(),
        }
    }

    /// Gives the calling thread its recorder, or `None` when all slots are taken.
    pub fn register(&self, role: ThreadRole) -> Option<Recorder> {
        // Relaxed: the counter only hands out unique indices; the ring itself is published
        // through the `OnceLock`.
        let index = self.registered.fetch_add(1, Ordering::Relaxed);
        let slot = self.threads.get(index)?;
        let ring = Arc::new(Ring::new(self.capacity_log2));
        let recorder = Recorder {
            ring: Arc::clone(&ring),
            epoch: self.epoch,
            single_producer: PhantomData,
        };
        slot.set(Registered { role, ring })
            .unwrap_or_else(|_| unreachable!("fetch_add hands out each index once"));
        Some(recorder)
    }

    /// Snapshots every ring while recording continues.
    pub fn dump(&self) -> Dump {
        let threads = self
            .threads
            .iter()
            .filter_map(OnceLock::get)
            .map(|t| {
                let (words, pushed) = t.ring.snapshot();
                let events: Vec<Event> = words.into_iter().map(Event::decode).collect();
                ThreadDump {
                    role: t.role,
                    lost_events: pushed - u64::try_from(events.len()).unwrap_or(pushed),
                    events,
                }
            })
            .collect();
        Dump { threads }
    }
}

/// Writes one thread's events. It can move to another thread but cannot be shared, which keeps
/// each ring single-producer.
pub struct Recorder {
    ring: Arc<Ring>,
    epoch: Instant,
    single_producer: PhantomData<Cell<()>>,
}

impl Recorder {
    /// Records an event stamped with the current time.
    pub fn record(&self, kind: EventKind, frame: u32, arg: u64) {
        let ts_ns = u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.record_at(ts_ns, kind, frame, arg);
    }

    /// Records an event with an explicit timestamp (synthetic sessions, replays).
    pub fn record_at(&self, ts_ns: u64, kind: EventKind, frame: u32, arg: u64) {
        let event = Event {
            ts_ns,
            frame,
            kind,
            arg,
        };
        self.ring.push(event.encode());
    }
}
