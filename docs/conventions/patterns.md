# Canonical Patterns

Recurring problems have **one** project-wide solution. When writing one of these, follow the
pattern; when a pattern does not fit, say why in the PR (and propose a change here). Ids
(`PAT-xx`) are cited in reviews.

The skeletons show **shape**, not final APIs: the crates they reference (`lince-sched`,
`lince-trace`, …) do not exist yet and will define the real signatures. Once a pattern has a
real implementation, this document links to it.

## PAT-01 — Error enum per crate

```rust
//! error.rs
use thiserror::Error;

/// Errors produced by the cache store. Never fatal to emulation (ARCH-17).
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("record at segment {segment} offset {offset} failed validation")]
    InvalidRecord { segment: u32, offset: u64 },
    #[error("store is locked by another process")]
    Locked,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = CacheError> = std::result::Result<T, E>;
```

Rules: one enum per crate (or per major module), variants carry the values needed to debug,
messages follow QUAL-42, no `String` catch-all variants.

## PAT-02 — Newtype for units and addresses

```rust
/// A guest CPU virtual address (39-bit address space on Switch 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct GuestVa(u64);

impl GuestVa {
    pub const fn new(raw: u64) -> Self { Self(raw) }
    pub const fn get(self) -> u64 { self.0 }
    #[must_use]
    pub const fn wrapping_add(self, bytes: u64) -> Self { Self(self.0.wrapping_add(bytes)) }
}
```

Rules: no `Deref` to the inner value; arithmetic only through named methods; `#[repr(transparent)]`
when crossing FFI or GPU buffers.

## PAT-03 — Decoding raw guest values at the boundary

```rust
/// Decodes the raw 3-bit primitive topology field. Unknown values are kept, not guessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topology { Points, Lines, Triangles, Unknown(u32) }

impl From<u32> for Topology {
    fn from(raw: u32) -> Self {
        match raw {
            0 => Self::Points,
            1 => Self::Lines,
            4 => Self::Triangles,
            other => Self::Unknown(other), // UNKNOWN: see research/ampere-3d-methods.md
        }
    }
}
```

Rules: raw integers are decoded once, at the edge; unknown encodings are represented
(`Unknown(raw)`), logged once through the ledger, and never silently mapped to a default.

## PAT-04 — Background job (compile workers, cache I/O, maintenance)

```rust
// Submitted through lince-sched, never std::thread::spawn (ARCH-04).
let job = Job::new(Priority::P1, "tier1-compile", move |cx: &JobContext| {
    if cx.is_cancelled() { return; }            // obsolete speculative work
    let pipeline = compile_tier1(&snapshot)?;   // reads only the immutable snapshot (ARCH-11)
    slot.publish(pipeline);                     // PAT-05
});
scheduler.submit(job);
```

Rules: jobs own their inputs (immutable snapshots or `Arc`s); they check cancellation between
stages; they never block on render-critical threads; results are published, not returned to a
waiting caller.

## PAT-05 — Publication slot (tier-up without blocking)

```rust
/// A pipeline slot read every draw by the GPU back thread and replaced by compile workers.
pub struct PipelineSlot {
    current: arc_swap::ArcSwap<PipelineHandle>,
}

impl PipelineSlot {
    /// Called on the render-critical path: one atomic load, never blocks.
    pub fn current(&self) -> arc_swap::Guard<Arc<PipelineHandle>> { self.current.load() }

    /// Called by compile workers. Release ordering is provided by ArcSwap's store.
    pub fn publish(&self, handle: PipelineHandle) { self.current.store(Arc::new(handle)); }
}
```

Rules: readers never take a lock; old handles are destroyed through deferred destruction tied
to the GPU timeline, not when the last `Arc` drops on a render thread.

## PAT-06 — Stage-to-stage queue on critical paths

```rust
// Bounded single-producer/single-consumer ring between GPU front and GPU back.
let (mut tx, mut rx) = ring::bounded::<MethodBatch>(RING_CAPACITY);

// Producer: on full, record a ledger event and park (backpressure), never grow.
while let Err(batch_back) = tx.try_push(batch) {
    ledger.event(Event::RingFull { ring: RingId::FrontToBack });
    batch = batch_back;
    tx.park_until_space();
}
```

Rules: bounded capacity chosen and documented; full/empty transitions are ledger events; no
`Mutex<VecDeque>` on critical paths; `loom` tests for any custom ring (DEV-32).

## PAT-07 — Frame-ledger instrumentation

```rust
// Zero-allocation scoped event on a critical thread.
let _span = ledger.span(EventKind::PipelineResolve { key });
// … work …
// span end is recorded on drop
```

Rules: every operation that can take more than ~10 µs on a render-critical thread is a span;
event payloads are fixed-size (no strings); names come from `EventKind`, never free text.

## PAT-08 — Platform abstraction

```rust
// src/platform.rs
#[cfg(windows)] mod windows;
#[cfg(windows)] use windows as imp;
#[cfg(unix)] mod unix;
#[cfg(unix)] use unix as imp;

/// Reserves `len` bytes of address space without committing memory.
pub fn reserve(len: usize) -> Result<Reservation> { imp::reserve(len) }
```

Rules: the platform module exposes one safe, neutral API; both implementations are compiled in
CI (ADR-0003); `unsafe` stays inside `imp`.

## PAT-09 — HLE service command handler

```rust
/// Example service — command table generated from the service description.
impl ExampleService {
    /// Cmd 3: returns the service interface version.
    fn get_version(&mut self, _ctx: &mut IpcContext) -> HleResult<u32> {
        Ok(EXAMPLE_VERSION) // research/nx2-services.md#example-get-version
    }
}
```

Rules: one method per command, named after the **publicly documented** name when one exists
(e.g. switchbrew), otherwise a descriptive name chosen by the project — never names from
leaked SDK material (LEGAL-12); unknown commands return the distinct "unimplemented command"
result and a ledger event (ARCH-31); guessed fields are marked `// GUESS:` with evidence.

## PAT-10 — Adding a guest shader instruction

1. Decoder entry in the instruction table (`lince-shader-sass-ampere/src/generated/` or the
   table source), with encoding evidence in `docs/research/ampere-sass-encoding.md`.
2. IR lowering in one function per instruction family.
3. Tier-0 interpreter support (or explicit no-fallback classification).
4. SPIR-V emission.
5. Differential test: Tier 0 vs Tier 1 vs reference output, bit-exact (ARCH-14).

All five in the same PR; never decoder-only additions.

## PAT-11 — Configuration setting

```rust
/// A typed setting with a default, a range and documentation (DEV-71).
pub const COMPILE_WORKERS_MAX: Setting<u32> = Setting::new("sched.compile_workers_max")
    .default(0) // 0 = decided by the governor
    .range(0..=64)
    .doc("Upper bound for compile workers; the governor may use fewer.");
```

Rules: no environment variables for behavior; settings are passed into subsystems at
construction (QUAL-53).
