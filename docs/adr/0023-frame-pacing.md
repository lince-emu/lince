# ADR-0023: Frame pacing and presentation

- **Status:** Proposed (needs PoC-10)
- **Date:** 2026-11-13
- **Related:** doc 01 E21; doc 05; ADR-0022

## Context
Unpaced presents let the guest run far ahead and desync (Lichii rows 55, 71); compile stalls
then lengthen fence waits (row 59). Good pacing hides small jitter; bad pacing creates it.

## Requirements
Emulated vsync at the guest's expected rate; host presentation aligned to display refresh;
bounded latency; VRR-friendly; correct behavior when the host is slower than the guest.

## Alternatives
1. Present immediately, no pacing.
2. **Present-wait based pacing** (`VK_KHR_present_id`/`present_wait`) with a target
   presentation time per guest frame; FIFO or FIFO_RELAXED; mailbox only for uncapped modes.
3. Pace on CPU timers only.

## Decision
Option 2 with timer fallback when present-wait is missing. At most 2 guest frames in flight.
Guest vsync events are generated from the emulated clock, independent of host refresh; host
presents are scheduled to the nearest refresh with VRR when available. Fence waits for guest
frame completion are bounded and recorded.

## Technical justification
Present-wait gives accurate feedback to align presentation and limit queue depth.

## Advantages
Lower, stable latency; consistent frame cadence.

## Disadvantages
Extension availability varies; fallback is less precise.

## Risks
Mismatch between guest 60 Hz and host 144 Hz without VRR causes judder. Mitigation: frame
duplication cadence control; documented recommendation for VRR.

## Validation plan
PoC-10: present-to-present interval distribution vs. guest frame times; external capture
(PresentMon) agreement.

## Conditions for revisiting
Present-wait unavailable on a large share of tier-1 hosts.
