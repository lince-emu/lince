# Ryujinx (Ryubing) guest kernel thread scheduler

Task context: T-051 (guest threads on six core threads, simple scheduler).
Repository: https://git.ryujinx.app/projects/Ryubing/ (Ryubing fork of Ryujinx), branch: default
(master), commit `6af327bdd50366163d80bc692f6bf47ec1c1993e`, shallow sparse clone in a scratch
dir outside the Lince tree. Paths below are relative to `src/Ryujinx.HLE/HOS/Kernel/`.
Behavior only; no code copied. This models the Switch 1 kernel (4 cores in practice), not NX2.

## Question

How does Ryujinx schedule guest kernel threads: run queues, priorities, preemption, yield
variants, core migration, idle handling, and the mapping of guest cores to host threads?

## Short answer

Ryujinx does NOT run one host thread per guest core. It runs one host thread per guest thread
(plus a preemption thread). Each guest core has only a scheduler object (selected thread, "needs
scheduling" flag, idle state). Threads block on a per-thread event until the scheduler selects
them for a core; at most one guest thread per core is released at a time. Selection is a
global, deterministic pass over per-core, per-priority FIFO queues (64 priorities, lower number
is better), under one global critical section, with load balancing from "suggested" queues
(threads whose affinity allows the core but which are queued elsewhere). Time slicing is a
10 ms ticker that rotates only one priority per core. For Lince (guest cores = host core threads)
the queue and selection semantics transfer; the host-thread-per-guest-thread handoff does not.

## Findings

| Source (repo@commit:file) | Observation | Level | Applies to Lince? |
|---|---|---|---|
| Ryubing@6af327b:Threading/KScheduler.cs | 64 priority levels (0 best). One scheduler object per core; state = selected thread, needs-scheduling flag. | SRC | Yes: same model as the guest kernel. |
| :Threading/KPriorityQueue.cs | Two queue sets indexed [priority][core]: "scheduled" (thread's active core) and "suggested" (other cores in its affinity mask). Per-core bitmasks of non-empty priorities give O(1) best-priority lookup. | SRC | Yes: good structure for a Lince `lince-kernel` run queue. |
| :Threading/KThread.cs (SetSchedFlags region, ~L1100-1200) | A running thread is placed in "scheduled" of its ActiveCore and "suggested" of every other core in its affinity mask. Leaving Running removes it from all. Some paths insert at queue head (prepend) instead of tail (e.g. a thread that was preempted keeps its place). | SRC | Yes. |
| :Threading/KScheduler.cs SelectThreadsImpl | Reselection runs only if a "reselection requested" flag is set; for each core the head of its best-priority scheduled queue becomes the selected thread. Returns a bitmask of cores whose selection changed. | SRC | Yes: compute selection once, then signal only changed cores. |
| same, pinned threads | If the process has a pinned thread on that core and it is runnable (and not kernel-waiting / exception thread), it overrides the queue head; if pinned but not runnable, core selects nothing. | SRC | Partially: only when SVCs for pinning (exception/debug) are implemented. |
| same, load-balancing pass | For each core with no scheduled thread: take the best "suggested" thread that is not already the selected thread of its own core and move it to this core. If every candidate is already selected elsewhere, take the second thread of that source core's queue to run there and migrate the first one here (never leaves the source core idle). Threads with priority < 2 (kernel message dispatch) are never migrated. | SRC | Yes (idle-core stealing); the "<2 never migrates" rule applies if such priorities appear. |
| same, SelectThread / EnableScheduling | Under the critical section, changing selection sets the core's needs-scheduling flag. On critical-section exit, other cores are poked first (interrupt request on their current thread, wake the idle waiter), then the current core reschedules itself, which may block. | SRC | Yes: maps to "set per-core interrupt flag; core thread checks at the next SVC return / block boundary". |
| same, Schedule / PickNextThread | Context switch = host-level handoff: current thread resets its own wait event, releases its context lock, picks the selected thread, signals it, then waits. A selected thread's context must be lockable (not still running on another core); if not, the core goes to idle and is retried when the other core releases it. Loop re-checks selection if it changed mid-switch. | SRC | No for the mechanism (host thread per guest thread). Yes for the invariant: a guest thread context is owned by at most one core at a time, and a migrating thread must be fully stopped (state saved) on the old core before the new core resumes it. |
| same, idle handling | There is no guest idle thread. A core with nothing selected runs no thread; it parks in an "idle active" state with a spin-locked "signalled" flag. When another core changes selection or releases a context, idle cores are signalled and re-evaluate. Idle time is accumulated per core (exposed through the core-time info). | SRC | Yes: idle core thread blocks on a condvar or park; wake on selection change. Idle-time counter is reusable for `GetInfo`-style queries. |
| same, PreemptionThreadLoop / RotateScheduledQueue | A dedicated host thread, every 10 ms (sleep-based, not guest-time based), takes the critical section and for each core rotates the head of exactly one priority class to the tail: priority 59 on cores 0 to N-2, priority 63 on the last core (index CpuCoresCount-1). It also tries to pull one suggested thread of that priority onto the core (preferring ones scheduled least recently) and, if the best thread on the core is at or worse than that priority, one with better priority. Then requests reselection. | SRC | Partially: same real-kernel behavior (only those classes are time sliced; all others run until they block or yield). Lince should use a guest-time or core-local timer rather than a host sleep (determinism). Core-index to priority mapping is for a 4-core console; GUESS: NX2 differs. |
| same, Yield (SVC 0xB, timeout 0) | Moves the current thread to the tail of its priority queue on its active core; requests reselection only if the new head differs. No migration. Ignored when the thread is unschedulable or not Running. | SRC | Yes. |
| same, YieldWithLoadBalancing (timeout -1) | Same rotate, then looks for a suggested thread with priority equal or better to the current one (or not scheduled more recently than it) that is not the selected thread of its core; if found, it is migrated to this core. | SRC | Yes (optional in T-051; can degrade to plain yield with a documented deviation). |
| same, YieldToAnyThread (timeout -2) | Current thread is detached from its core (core set to none, so it sits only in suggested queues). If the core has no scheduled thread, steal a suggested thread (not the head of its own core, or whose core head has priority >= 2) and run it; reselect. | SRC | Yes, low priority for T-051. |
| Syscall.cs (SleepThread, SVC 0xB) | Timeout 0, -1, -2 select the three yield flavors; positive timeouts put the thread to sleep with a timer (a small constant is added to the timeout). Other negative values do nothing. | SRC | Yes. |
| :Threading/KThread.cs SetCoreAndAffinityMask (~L789-840) | Setting affinity: preferred core must be inside the mask (else error); if the active core leaves the mask, the active core becomes the preferred core, or the highest set bit if the preferred is "any" (-1); then queues are rebuilt for the new mask. Migration-disabled threads keep the original values and apply on re-enable. | SRC | Yes: needed for SVC SetThreadCoreMask. |
| :KernelContext.cs (L66, L99-120) | Core count taken from the device config; one scheduler per core; one `HLE.PreemptionThread` started at kernel creation. | SRC | Yes. |
| :Threading/KThread.cs (ThreadStart, ~L1302) | Every guest thread owns a host thread that first waits on its scheduler-wait event, then executes guest code; exit makes it unschedulable and the next thread is signalled. | SRC | No (Lince has fixed core threads). |
| :Threading/KThread.cs (L1054-1080) | Priority inheritance for mutex waiters: dynamic priority is the best among the base priority and waiters; waiter list ordered by dynamic priority. | SRC | Yes, if T-051 includes mutex/arbiter SVCs; otherwise defer. |

## Inferences (marked as such)

- INFERENCE: coarse global critical section plus a "reselect once on exit" flag is the key to
  determinism; every state change (wake, priority, affinity, yield) only mutates queues and
  sets the flag, and one pass assigns all cores. Lince can copy that shape.
- INFERENCE: the 10 ms sleep-based quantum is an emulator choice; real behavior depends on the
  hardware timer, so a guest-tick-based quantum is a legitimate deviation.
- INFERENCE: the six-core Lince layout makes the 4-core preemption-priority table (59 / 63)
  unverified for NX2; keep it a configurable table.

## Open questions

1. Which core (and which priority) gets the 63 slice on NX2 with a different core count? Needs
   a non-Ryujinx source (homebrew measurement on hardware, D-xxx).
2. Should T-051 implement the suggested-queue load balancing, or only fixed-core affinity?
   (Simplest: scheduled queues only, plus idle-core stealing later.)
3. Handling of a selected thread whose context is still running on another core (retry vs
   spin vs block) must be designed for Lince's real parallel core threads.
4. Preemption timer source (guest ticks vs a host timer thread under `lince-sched`).
5. This note covers neither the address arbiter, condition variables nor process exit.

## Affected ADRs, PoCs, backlog

- T-051 (guest threads / simple scheduler): use [priority][core] FIFO queues with a
  non-empty-priority bitmask, global lock, one reselect pass, per-core interrupt flag.
- Likely needs an ADR (or a design note under `docs/02`-`docs/05`) for: thread-to-core handoff
  protocol, preemption timer source, deviation from Ryujinx on host-thread model.
- Related: `docs/research/b2-svcs-and-ipc.md` (SVC list), `docs/research/cpu-jit-scheduling.md`
  (host scheduling), ADR on `lince-sched` priority classes (core threads must not be ad hoc).
