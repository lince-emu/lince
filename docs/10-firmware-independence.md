# 10 — Firmware and Key Independence

Status: Proposed plan under an Accepted decision (ADR-0029) · Last reviewed: 2026-11-13

## 1. The guarantee

**Lince-NX runs games without console keys and without console firmware — always.**
There is no "firmware installed" mode, no key file, no detection of either, and no feature
that becomes available when they are present. The firmware-free path is the only path.

## 2. What this means for content (stated plainly)

| Content | Runs? | Why |
|---------|-------|-----|
| Unencrypted executables and data (loose ExeFS + RomFS, the layout used for development) | Yes | Nothing to decrypt |
| Homebrew built for the console | Yes, once its formats are supported | Homebrew is not encrypted |
| Retail packages as distributed (NSP, XCI, NCA, …) | **No** | They are encrypted; Lince-NX never decrypts (ADR-0029, LEGAL-02) |

Lince-NX does not tell users how to convert encrypted content and does not link to tools that
do (LEGAL-03, LEGAL-04). The loader reports encrypted input with a neutral message.

## 3. What games expect from the system, and how each need is met without firmware

Games call system services and read system data. Every item below is provided by an original
implementation or a freely licensed substitute. Evidence levels as in doc 01: [EMU] observed by
other emulator projects, [RECALL] known from Switch 1 emulation history, not re-verified,
[Inferred], [Unknown].

| Need | Typical use by games | Lince-NX approach | Evidence |
|------|---------------------|-------------------|----------|
| Kernel and SVCs | Threads, memory, synchronization | Original HLE kernel (ADR-0024) | EMU |
| System services (IPC) | Filesystem, HID, audio, graphics system, time, accounts, settings | Original HLE services, declarative command tables (ADR-0024) | EMU |
| Graphics system services (`gsv`, `vi2`, syncpoints, GPU memory) | Boot NVN2, present frames | Original HLE (doc 08 §3) | EMU |
| Shared system fonts | UI text rendering | Freely licensed fonts with compatible metrics; differences documented | EMU (Lichii row 36), RECALL |
| Time-zone data | Local time display | Generated from IANA tzdata (public domain) in the format the service returns | RECALL |
| System version information | Feature checks | HLE returns a fixed, documented version profile per supported SDK range | Inferred |
| Profanity filter data (`ngc`-style services) | Filtering user text | Empty or permissive list; documented behavior | RECALL |
| Avatar/character model data used by some games | Player avatars | Original placeholder data or feature reported as unavailable | RECALL |
| System applets (software keyboard, error dialog, user selector, controller setup, web) | UI flows launched by games | Original HLE applets with Lince-NX's own UI | EMU (keyboard input in Lichii notes), RECALL |
| Certificates and network services | Online features | Not provided; network features report offline (doc 07 M13) | Inferred |
| Firmware-only Switch 2 system data | — | **Unknown**; each case found is added to this table and handled by an original implementation or left unimplemented, never with firmware (LEGAL-06) | Unknown |

## 4. Rules for implementers

1. A missing system feature is implemented originally, substituted with freely licensed data,
   or reported as unavailable through the correct service error — never satisfied with
   firmware data (LEGAL-06).
2. Substitute data lives in `assets/system/` with its license recorded next to it (LEGAL-34).
3. Every system-data format implemented is documented in `docs/research/` from public or
   independently observed behavior (LEGAL-11 to LEGAL-13).
4. Tests boot titles and homebrew in the only mode that exists: no keys, no firmware.

## 5. Open work

- T-019: research which system data and applets Switch 2 titles depend on, from public sources
  and observed service calls, and extend §3.
