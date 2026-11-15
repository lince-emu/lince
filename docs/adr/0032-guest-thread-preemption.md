# ADR-0032: Horizon-faithful guest thread preemption

- **Status:** Accepted: the recommended option of each sub-decision, 1b, 2c, 3c, 4a and 5c
  (maintainer, 2026-11-14).
- **Date:** 2026-11-14
- **Deciders:** maintainer
- **Related:** ADR-0004, ADR-0017, ADR-0018, ADR-0024; `docs/research/ryujinx-kernel-scheduler.md`,
  `docs/research/yuzu-prometheus-scheduler.md`; T-051 (PR #59), T-061

## Context
T-051 (PR #59) runs guest threads on the six core threads (ADR-0017 B2 scope) with a simple
scheduler: each core runs its best-priority ready thread, every priority level takes turns
every 10 000 instructions, there is no migration, and a thread made ready by another core
waits for the running thread's slice to end. The scheduler study shows that the real kernel
behaves differently (Ryujinx note, rows `PreemptionThreadLoop`, `SelectThreadsImpl`,
`EnableScheduling`, `Yield*`; behavior only, SRC level for the Switch 1 kernel):
- only **one priority class per core** is time sliced, every 10 ms: priority 59 on cores 0 to
  N-2 and 63 on the last core of a 4-core console; every other priority runs until it blocks
  or yields;
- a selection change **interrupts** the affected core promptly instead of waiting;
- threads are balanced across cores through "suggested" queues (idle-core stealing, migration
  on rotation, the `SleepThread` -1 and -2 yield variants).

Guest programs can depend on this (a priority-44 spin loop that must not be time sliced; a
high-priority wake that must run now). No Accepted ADR covers the rules, the time source, or how
a thread's state moves between parallel core threads. **UNKNOWN:** the sliced priorities and
the quantum on 6-core NX2 (doc 08 §4); yuzu's blog says nothing on queues (yuzu note).

## Requirements
Guest-visible scheduling matches Horizon where known, and every guess is a configurable value;
no ad-hoc threads (ARCH-04, invariant 7); no I/O on core threads (ARCH-01); a guest thread's
state is owned by at most one core at a time; kernel tests can drive the quantum by hand.

## Alternatives and decision
Each sub-decision lists its options; **bold** is the recommendation, accepted by the
maintainer (2026-11-14).

1. **Sliced priorities.** (a) Status quo: every priority sliced; fair, but unfaithful.
   **(b) Per-core table** `[Option<u32>; 6]`, one rotated priority per core or none; default
   `59, 59, 59, 59, 59, 63`, which extends the 4-core rule (GUESS until measured on NX2).
   (c) A set of priorities per core: more general, and no evidence needs it.
2. **Prompt preemption.** (a) Status quo: wait for the slice; with (1b) a non-sliced thread
   would never leave its core, so it is no longer an option. (b) Run guest code in short chunks
   and poll a flag between them: no `lince-cpu-a64` change, but latency equals the chunk and
   every chunk retakes the address-space read lock (T-060). **(c) A per-core `AtomicBool`
   "reschedule requested"** that `lince_cpu_a64::run` checks at block boundaries and that
   makes it return a new `Exit::Interrupted`; the same contract a JIT block exit needs. It is a
   public API change of `lince-cpu-a64` (AGENTS.md §4). Whoever changes a core's selection (see
   4) sets the flag and unparks the core; the core reselects at its next safe point (block
   boundary or SVC return).
3. **Quantum time source.** (a) Guest instructions per core (status quo): no thread, but it
   drifts from wall time and from other cores, and differs between interpreter and JIT (yuzu
   note, cycle timers). (b) A core-local deadline checked against the host monotonic clock at
   safe points: no thread, but a clock read on the hot path, and rotation still needs
   cross-core work under the lock. **(c) One kernel timer role thread owned by `lince-sched`**
   that every quantum (default 10 ms, GUESS from the Ryujinx note) takes the kernel lock,
   rotates the table's priorities and sets the flags of changed cores; it also takes over
   sleep and timeout wakeups. It extends the ADR-0017 B2 scope by one role thread. Tests
   replace it with a manual tick.
4. **Thread-to-core handoff.** **(a) One kernel lock and one reselection pass** (Ryujinx note):
   every state change only edits queues and sets "reselect"; one pass under the lock assigns
   each core its thread and returns the mask of changed cores, which get their flag set. A
   thread's state is either in the table or held by exactly one core (`Running { core }`), and
   moves only under the lock, as `pick`/`put_back` do in T-051. A thread selected for core B
   while still running on A is not taken by B; A gives it back at its next safe point and the
   pass runs again. (b) Per-thread context locks: less contention, but lock ordering and
   deadlock risk with no measurement asking for it.
5. **Suggested-queue load balancing.** (a) None: run only on the ideal core; -1 and -2 yield
   like 0 (documented deviation). (b) Full: suggested queues, idle-core stealing, migration on
   rotation, -1/-2 variants, priorities below 2 never migrate. **(c) Staged:** T-061 implements
   (1b)–(4a) with no migration but keeps the queue layout `[priority][core]` for both
   "scheduled" and "suggested" so (b) adds no new protocol; (b) becomes its own task when a
   title idles cores while threads queue elsewhere.

## Technical justification
(1b) is the only known-faithful rule and stays configurable for NX2. (2c) bounds latency by a
block, not by a slice, and is what a JIT must do anyway. (3c) puts the global rotation where
the global lock is, keeps the core loop free of clock reads, and follows ARCH-04. (4a) is
T-051's ownership rule extended to cross-core selection, so it adds the least new code.

## Advantages
Faithful slicing and preemption; one place (the reselection pass) decides every core; the
guesses are data, not code.

## Disadvantages
A `lince-cpu-a64` API change; one more role thread; a global lock that every reselection takes.

## Risks
The default table is wrong on NX2: a measured value changes the default, not this ADR. Kernel
lock contention with six busy cores: measured by the ledger, (4b) as the fallback. Tests or
homebrew that relied on T-051's fairness change behavior: listed in T-061.

## Validation plan
Kernel tests in T-061: two priority-44 threads on one core, one spinning, the other never runs
until it yields; two priority-59 threads alternate each manual tick; a priority-0 thread woken
from core 1 interrupts a spinner on core 0 before its next block ends; `SetThreadCoreMask` on
a running thread never leaves its state on two cores (loom model of the handoff). Ledger:
preemption latency (flag set to switch) and kernel lock hold time per core thread.

## Conditions for revisiting
An NX2 measurement contradicts the table or the quantum (change the default); preemption
latency P99 > 200 µs (ADR-0017 threshold); kernel lock hold time P99 > 50 µs (GUESS threshold) or wait time
dominating a core's ledger (consider 4b); a title stalls with idle cores and queued threads
(start load balancing, 5b).
