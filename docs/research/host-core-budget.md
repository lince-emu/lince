# Study: host core budget (T-037)

- **Task:** T-037. **Date:** 2026-11-14. **ADRs:** 0005, 0017, 0018.
- **Sources:** Lince documents only (doc 04 §3, ADR-0005, ADR-0017, ADR-0018, doc 06 PoC-8).
  No other project was read. Nothing here has been measured, so every number is a GUESS until
  PoC-8 and the ledger confirm it (AGENTS.md §3: no unmeasured performance claims).

## 1. Question

ADR-0005 moves all JIT work to `lince-sched` workers. Those workers compete with 6 guest core
threads and the role threads (GPU front, GPU back, Vulkan submit, present, audio). How many
threads does each role need? How should the priority classes and the governor share 4–16
host cores? What happens when the host has fewer cores than the guest? What must the ledger
record to check the answers?

## 2. Short answer

- **Thread count is not the problem; runnable threads are.** Doc 04 §3 adds up to about
  12 critical threads plus workers. Guest cores park while the guest waits, and present and
  audio sleep most of a frame. The budget is therefore *busy* critical threads: the active
  guest cores plus the GPU pipeline threads, measured every frame.
- **Proposed policy:** keep `active guest cores + busy pipeline threads` on dedicated
  physical cores, and give the governor whatever is left for workers (table in §4). P0 always
  gets one worker. Every other class shrinks to zero under deadline pressure.
- **Fewer host cores than guest cores:** keep the 1:1 mapping (ADR-0017), turn guest
  `WFE`/`YIELD` into host yields, and measure. An M:N guest-core scheduler is the fallback,
  and it would need an ADR-0017 change (§6).
- **Ledger:** add run-queue delay, guest-core park/unpark, governor output per class, and a
  startup host-topology record (§5).

## 3. Demand per role

"Busy" is the expected share of a frame the thread is runnable. Every value is a GUESS to be
replaced by ledger data.

| Role | Threads | Busy share (GUESS) | Notes |
|------|---------|--------------------|-------|
| Guest core | 6 | 0–1 each; the title decides | Parks on guest wait SVCs and `WFI`. How many cores Switch 2 titles keep busy is UNKNOWN (doc 08). |
| GPU front | 1 | 0.3–0.8 | Busy in proportion to guest command volume. |
| GPU back | 1 | 0.3–0.8 | Host command recording. |
| Vulkan submit | 1 | < 0.2 | Mostly waits on the driver. |
| Present | 1 | < 0.05 | Wakes once per vsync. |
| Audio | 1 | < 0.05 | Short periodic bursts; latency matters, throughput does not. |
| HLE service hosts | few | low | Blocking service calls only. |
| Cache I/O + writer | 1–2 | low | Only when the governor allows it (doc 02). |
| Workers (JIT, shaders, prefetch) | governed | what is left | ADR-0005 Tier B/C; doc 03 Tier-1; doc 02 P2 prefetch. |

**Critical demand** `D = active guest cores + busy(front) + busy(back) + busy(submit)`, taken
as a moving percentile over recent frames. Present and audio need low latency, not cores.
They are protected by OS priority (doc 04 §3), not by core reservation.

## 4. Proposed policy per host core count

Rules:
1. Count **physical** cores for critical demand. SMT siblings run workers only, because two
   critical threads on one physical core slow each other down.
2. On hybrid CPUs, critical threads prefer performance cores and workers prefer efficiency
   cores. This is an affinity or QoS hint through the platform layer, never a hard pin.
3. Worker budget `W = physical cores − ceil(D) + SMT siblings`, clamped to at least 1. The
   governor (ADR-0018) splits `W` across classes. P0 is always allowed 1 worker even when
   `W` is 1, because a missing P0 result costs the current frame (Tier-0 exposure). P1 keeps
   a floor of 1 while there is headroom. P2–P4 drop to 0 under pressure.
4. The governor drops a class's concurrency when critical run-queue delay rises, not when
   the CPU is busy. Busy cores are fine as long as critical threads start on time.

| Physical cores (SMT) | Typical D (GUESS) | Workers at headroom | Workers under pressure | Expected risk |
|----------------------|-------------------|---------------------|------------------------|---------------|
| 4 (8) | 3–5 | 1 + 4 SMT | 1 (P0 only) | Fewer cores than active guest cores; §6 applies |
| 6 (12) | 4–6 | 1–2 + 6 SMT | 1 | Tier-up latency grows; guest stays interpreted longer |
| 8 (16) | 4–6 | 2–4 + 8 SMT | 1–2 | Target class; PoC-8 measures here |
| 12+ (24+) | 4–6 | 6+ | 2+ | Memory bandwidth, not cores, limits workers (ADR-0018) |

## 5. What the ledger must record

The existing kinds already cover part of this: `ThreadPreempted{ns}` and
`GovernorThrottle{level}` (doc 05 §3). Proposed additions, with raw values chosen when they
are implemented (T-030 already uses 22 and 23):

| Event | Recorded by | Argument | Answers |
|-------|-------------|----------|---------|
| `RunQueueDelay` | every critical thread, sampled | ns from wake-up to running | Are critical threads starved? (main governor input) |
| `GuestCorePark` / `GuestCoreUnpark` | guest core | park reason | Active guest cores per frame, i.e. `D` |
| `WorkerConcurrency` | governor | class << 32 \| allowed workers | What the governor chose, to compare with run-queue delay |
| `GuestYield` | guest core | count per frame | How much guest spinning there is (guest `WFE`/`YIELD`) |
| `HostTopology` | startup, once | physical, logical, P and E core counts | Puts each dump in its host class |

Validation: on 6- and 8-core hosts, P99.9 `RunQueueDelay` of critical threads stays below a
budget set by PoC-8, while P0 and P1 completion times stay bounded. These are the PoC-8
success criteria, applied per host class.

## 6. Fewer host cores than guest cores

- **1:1 with oversubscription (current ADR-0017).** The OS time-slices 6 guest threads on 4
  physical cores. The risk is guest spin-waits burning a whole time slice while the lock
  holder is descheduled. Mitigations: map guest `WFE`/`YIELD` to a host yield, and park on
  guest wait SVCs instead of spinning. Cheap to do, but whether it is enough is UNKNOWN.
- **M:N guest cores (proposed fallback).** Fewer host threads than guest cores, with a
  Lince-owned scheduler that switches guest contexts at block exits, SVCs and yields. This
  gives deterministic switching and no lock-holder preemption, but it is a kernel-scheduler
  feature in `lince-kernel` plus a change to ADR-0017 ("Guest cores are real host threads
  (6)").
- **Recommendation:** ship 1:1 with yield mapping. Build M:N only if the ledger shows high
  `GuestYield` together with high `RunQueueDelay` on 4-core hosts.

## 7. Proposed ADR changes (not applied)

1. **ADR-0017:** define P0–P4 in one place. Proposal: P0 needed for the current frame, P1
   tier-up of running code, P2 speculative/prefetch, P3 cache maintenance, P4 idle-only.
   Today they appear in ADR-0018, doc 02 and doc 03 without one definition.
2. **ADR-0017:** add the core-budget rules of §4 (physical cores for critical demand, SMT and
   E-cores for workers, P0 floor of 1).
3. **ADR-0017:** allow "up to 6 guest threads, M:N below a host core threshold" as a future
   option, decided by the §6 ledger evidence. Keep 1:1 until then.
4. **ADR-0018:** name `RunQueueDelay` of critical threads as the main governor input, and
   add `WorkerConcurrency` as its recorded output.
5. **Doc 05 §3:** add the §5 events to the ledger list.

## 8. Open questions

- How many guest cores Switch 2 titles keep busy (UNKNOWN, doc 08). This drives `D` more
  than any Lince choice.
- Whether OS priorities and hybrid-core QoS hints are honored without elevated rights on each
  OS (doc 04 §3 already calls them hints, measured in PoC-8).
- Whether memory bandwidth, not cores, caps useful workers on 8+ core hosts (ADR-0018 notes
  bandwidth contention). Only PoC-8 can tell.
