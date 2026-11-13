// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Single-producer ring of fixed-size events, readable concurrently (seqlock per slot).
//!
//! Each slot is four `AtomicU64`: a sequence word and the three event words. To write event
//! number `n`, the producer stores `2n + 1` (odd: being written), issues a release fence,
//! stores the data, then stores `2n + 2` with release ordering. A reader accepts slot data only
//! if it read `2n + 2` before and after the data (with an acquire fence in between); anything
//! else is an event that is being written or was overwritten, and is skipped. This is the
//! fence-based seqlock described by H.-J. Boehm, "Can Seqlocks Get Along with Programming
//! Language Memory Models?" (MSPC 2012).
//!
//! Threads: `push` runs on the owning (possibly render-critical) thread and never blocks or
//! allocates. `snapshot` allocates and copies, so it runs off render-critical threads.

// The loom model (`loom_tests` below) swaps in loom's atomics; every other build uses std's.
#[cfg(all(test, loom))]
use loom::sync::atomic::{AtomicU64, fence};
use std::sync::atomic::Ordering;
#[cfg(not(all(test, loom)))]
use std::sync::atomic::{AtomicU64, fence};

const WORDS_PER_SLOT: usize = 4;

/// The ring. Only one thread may call [`Ring::push`]; any thread may call [`Ring::snapshot`].
pub(crate) struct Ring {
    slots: Box<[AtomicU64]>,
    mask: u64,
    /// Number of events ever pushed.
    head: AtomicU64,
}

impl Ring {
    /// A ring holding the last `2^capacity_log2` events.
    pub(crate) fn new(capacity_log2: u32) -> Self {
        let capacity = 1usize << capacity_log2;
        Self {
            slots: (0..capacity * WORDS_PER_SLOT)
                .map(|_| AtomicU64::new(0))
                .collect(),
            mask: (1u64 << capacity_log2) - 1,
            head: AtomicU64::new(0),
        }
    }

    fn slot(&self, n: u64) -> &[AtomicU64] {
        let index = usize::try_from(n & self.mask).unwrap_or(0) * WORDS_PER_SLOT;
        &self.slots[index..index + WORDS_PER_SLOT]
    }

    /// Appends an event, overwriting the oldest one when full. Never blocks.
    pub(crate) fn push(&self, words: [u64; 3]) {
        // Relaxed: only this (single) producer ever stores `head`.
        let n = self.head.load(Ordering::Relaxed);
        let slot = self.slot(n);
        slot[0].store(2 * n + 1, Ordering::Relaxed);
        fence(Ordering::Release);
        slot[1].store(words[0], Ordering::Relaxed);
        slot[2].store(words[1], Ordering::Relaxed);
        slot[3].store(words[2], Ordering::Relaxed);
        slot[0].store(2 * n + 2, Ordering::Release);
        // Release pairs with the Acquire in `snapshot`: every slot below `head` has its final
        // sequence word (or a later one) visible to a reader that sees this `head`.
        self.head.store(n + 1, Ordering::Release);
    }

    /// Copies the newest contiguous run of events, oldest first, and returns it with the number
    /// of events ever pushed up to the snapshot. An event overwritten or being written while the
    /// snapshot runs is never returned torn; it and every older event are left out, so the
    /// result never has a hole.
    pub(crate) fn snapshot(&self) -> (Vec<[u64; 3]>, u64) {
        // Acquire pairs with the Release store of `head` in `push`.
        let head = self.head.load(Ordering::Acquire);
        let first = head.saturating_sub(self.mask + 1);
        let mut out = Vec::with_capacity(usize::try_from(head - first).unwrap_or(0));
        for n in first..head {
            let slot = self.slot(n);
            let before = slot[0].load(Ordering::Acquire);
            if before != 2 * n + 2 {
                // Overwritten (or being written): drop everything older too, so the result is
                // always the contiguous newest run and `head - len` counts every missing event.
                out.clear();
                continue;
            }
            let words = [
                slot[1].load(Ordering::Relaxed),
                slot[2].load(Ordering::Relaxed),
                slot[3].load(Ordering::Relaxed),
            ];
            fence(Ordering::Acquire);
            if slot[0].load(Ordering::Relaxed) == before {
                out.push(words);
            } else {
                out.clear();
            }
        }
        (out, head)
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::Ring;

    #[test]
    fn keeps_the_newest_events_in_order_after_wrapping() {
        let ring = Ring::new(4);
        for i in 0..100u64 {
            ring.push([i, i * 2, i * 3]);
        }
        let (snap, pushed) = ring.snapshot();
        assert_eq!(pushed, 100);
        assert_eq!(snap.len(), 16);
        for (offset, words) in snap.iter().enumerate() {
            let i = 84 + offset as u64;
            assert_eq!(*words, [i, i * 2, i * 3]);
        }
    }

    #[test]
    fn empty_and_partial_rings() {
        let ring = Ring::new(4);
        assert_eq!(ring.snapshot(), (Vec::new(), 0));
        ring.push([1, 2, 3]);
        assert_eq!(ring.snapshot(), (vec![[1, 2, 3]], 1));
    }

    #[test]
    fn a_skipped_slot_drops_everything_older_so_the_result_has_no_hole() {
        use super::WORDS_PER_SLOT;
        use std::sync::atomic::Ordering;
        let ring = Ring::new(4);
        for i in 0..16u64 {
            ring.push([i, 0, 0]);
        }
        // Stand-in for a slot the producer overwrote while the snapshot was copying.
        ring.slots[5 * WORDS_PER_SLOT].store(0, Ordering::Relaxed);
        let (snap, pushed) = ring.snapshot();
        let kept: Vec<u64> = snap.iter().map(|w| w[0]).collect();
        assert_eq!(kept, (6..16).collect::<Vec<u64>>());
        assert_eq!(pushed, 16);
    }
}

/// Loom model of one producer and one concurrent snapshot (D-004, DEV-32). Run with
/// `RUSTFLAGS="--cfg loom" cargo test --release --lib loom`.
#[cfg(all(test, loom))]
mod loom_tests {
    use super::Ring;
    use loom::sync::Arc;
    use loom::thread;

    /// The words of event `n`. Every word differs between events and from the zeroed slots, so
    /// a torn copy, or one from another event, never matches.
    fn words(n: u64) -> [u64; 3] {
        [3 * n + 1, 3 * n + 2, 3 * n + 3]
    }

    /// One producer pushes events 1 and 2 while the model thread takes a snapshot.
    fn push_while_snapshotting() {
        // One slot, so every push overwrites the event the snapshot may be copying.
        let ring = Arc::new(Ring::new(0));
        ring.push(words(0));
        let producer_ring = Arc::clone(&ring);
        let producer = thread::spawn(move || {
            producer_ring.push(words(1));
            producer_ring.push(words(2));
        });
        let (snapshot, head) = ring.snapshot();
        // The seqlock property: the snapshot is the newest run of events below `head`, and each
        // entry is exactly the event with that number, never torn or stale.
        let len = u64::try_from(snapshot.len()).expect("a snapshot length fits in u64");
        assert!(len <= head);
        for (n, entry) in (head - len..).zip(&snapshot) {
            assert_eq!(*entry, words(n), "torn or stale entry for event {n}");
        }
        producer.join().expect("the producer thread does not panic");
    }

    #[test]
    fn a_concurrent_snapshot_returns_only_whole_events_under_their_own_number() {
        loom::model(push_while_snapshotting);
    }
}
