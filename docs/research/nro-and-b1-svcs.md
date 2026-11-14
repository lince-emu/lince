# Research: NRO format and the SVCs B1 uses (T-028)

- **Task:** T-028. **Date:** 2026-11-14.
- **Sources:** public Switchbrew wiki pages only (LEGAL-11). No SDK, header or leaked material
  was read. Wiki pages have no commit, so each row cites the URL and the access date
  (2026-11-14). The facts were read through a page-summarising fetch tool, not by reading the raw
  wiki text. Re-read the raw pages before T-029 and T-034 freeze their tests.
- **Licence note:** only facts (offsets, sizes, numbers) are transcribed, in our own words. The
  wiki's licence was not checked; do not paste its text or tables verbatim.

## 1. Question

What are the NRO header and segment layout, the entry state of a homebrew program, and the
numbers, arguments and results of the SVCs B1 uses (memory query/set, exit,
`OutputDebugString`)? What do public sources say about Switch 2 SVC `0x80`? What is Switch 1
knowledge and what is known to hold on Switch 2?

## 2. Short answer

- An NRO is `[16-byte start][NRO0 header at 0x10, 0x80 bytes][text][ro][data]`, with a BSS size
  but no BSS bytes in the file. Segments are described by (offset, size) pairs, and the file
  starts with a branch that skips the header (the entry point is file offset 0).
- A homebrew NRO is entered with `X0` = pointer to a loader-config list and `X1` =
  `0xFFFFFFFFFFFFFFFF`. B1 can pass an empty list (only an EndOfList entry), or add the
  MainThreadHandle and Argv entries.
- B1 needs only SVCs `0x01`, `0x02`, `0x03`, `0x06`, `0x07` and `0x27`. Their registers and the
  Switch 1 result codes are in section 3.
- SVC `0x80` is listed as `SetGpuMemoryAttribute` with the same arguments as `0x03`, tagged
  `[S2]`. The tag has no legend on the page, so reading it as "Switch 2" is an inference.
- Everything here is Switch 1 knowledge. Nothing is verified on Switch 2 (see section 4).

## 3. Findings

Evidence levels: DOC = public documentation, read, not reproduced. "Applies" = to Lince B1.

| # | Source | Observation | Level | Applies? |
|---|--------|-------------|-------|----------|
| N1 | switchbrew.org/wiki/NRO, 2026-11-14 | Offsets `0x00`-`0x0F`: start block. The module-header location sits here (12 bytes on 17.0.0+, 8 on earlier firmware); the last 4 bytes are reserved. | DOC | Yes |
| N2 | same | Header at `0x10`: magic `NRO0`; `+0x14` version; `+0x18` total file size; `+0x1C` flags (bit 0 AlignedHeader; bit 1 Compress, 23.0.0+). | DOC | Yes. B1 writes flags 0, version 0 (the version value is a guess, see O1) |
| N3 | same | `+0x20` text offset, `+0x24` text size, `+0x28` ro offset, `+0x2C` ro size, `+0x30` data offset, `+0x34` data size, `+0x38` bss size (all u32, offsets are file offsets and memory offsets). | DOC | Yes |
| N4 | same | `+0x3C` AdditionalHeaderOffset (23.0.0+); `+0x40` 32-byte module id (GNU build id); `+0x60` DsoHandleOffset; `+0x64` reserved; `+0x68`/`+0x6C` embedded offset/size; `+0x70`/`+0x74` dynstr offset/size; `+0x78`/`+0x7C` dynsym offset/size. Header ends at `0x80`. | DOC | Partly. B1 can zero everything except sizes and segment fields |
| N5 | same | Optional asset block follows the NRO at file offset = header size field. Magic `ASET`, version 0, then three (offset u64, size u64) sections: icon, NACP, romfs. Offsets are relative to the asset header. | DOC | No for B1. The loader must ignore bytes past the NRO |
| N6 | same | Additional header `NRA0` (23.0.0+) holds a compressed size and a hash. | DOC | No. Reject the Compress flag in B1 |
| N7 | switchbrew.org/wiki/MOD, 2026-11-14 | Location struct: `+0` version (17.0.0+; earlier padding), `+4` header offset, `+8` version offset. MOD0 header: magic, then dynamic, bss start, bss end, exception-info start/end, module-object offsets (u32 each); 19.0.0+ adds relro and build-id offsets. | DOC | Partly. Static B1 programs need no dynamic linking |
| N8 | MOD page vs NRO and ABI pages | The ABI page says file offset 0 is a branch; the MOD page says its offset 0 is a version word. The pages do not say what the MOD offsets are relative to, or how the two fit. This is inconsistent as summarised. | DOC, conflict | See O2 |
| E1 | switchbrew.org/wiki/Homebrew_ABI, 2026-11-14 | Entry as an NRO: `X0` = pointer to the loader config, `X1` = all ones. Entry as an NSO: `X0` = 0, `X1` = main-thread handle. | DOC | Yes |
| E2 | same | Config = array of 24-byte entries: u32 key, u32 flags (bit 0 = mandatory), two u64 values. Key 0 EndOfList ends it. Key 1 MainThreadHandle (required). Key 5 Argv (value 1 = string pointer). Key 7 AppletType (required). Key 3 heap override, key 10 process handle, key 16 HOS version. | DOC | Yes. B1 supplies keys 0, 1, 5 |
| E3 | same | Without an Argv entry the program sees argc 1 and an empty argv[0]. Returning goes to the original `LR`; if it is null the program calls ExitProcess. | DOC | Yes. B1 programs end with SVC `0x07` |
| S1 | switchbrew.org/wiki/SVC, 2026-11-14 | Result comes back in `W0`; arguments in `X0`-`X?` as listed below. The page does not state how the SVC number is encoded (a 64-bit program puts it in the `SVC` instruction immediate). | DOC; encoding is RECALL | Yes |
| S2 | same | `0x06` QueryMemory: `X0` = pointer to a 0x28-byte MemoryInfo, `X2` = address; out: `W0` result, `W1` page info. Returns the page-aligned mapping containing the address. MemoryInfo: base `+0x00` (u64), size `+0x08` (u64), type `+0x10`, attribute `+0x14`, permission `+0x18`, ipc refs `+0x1C`, device refs `+0x20`, pad `+0x24` (u32 each). Page info is never defined on the page. | DOC | Yes |
| S3 | same | MemoryType values (u32, partial): Free `0`, Io `0x2001`, Static `0x42002`, Code `0xDC7E03`, CodeData `0x3FEBD04` (4.0.0+), Normal `0x37EBD05` (4.0.0+), Stack `0x5C3C0B`, ThreadLocal `0x40200C`, Inaccessible `0x10`. The page notes the table was hard to read. | DOC, partly unclear | Yes. Check the raw page before T-034 |
| S4 | same | MemoryAttribute bits: 0 Locked, 1 IpcLocked, 2 DeviceShared, 3 Uncached, 4 PermissionLocked, 5 GpuSharable `[S2]`, 6 GpuShared `[S2]`. MemoryPermission bits: 0 R, 1 W, 2 X, 28 DontCare. | DOC | Yes |
| S5 | same | `0x01` SetHeapSize: `X1` = size; out `W0` result, `X1` = heap base. Size must be a multiple of 2 MiB; at most 4 GiB (2.0.0+); the kernel picks the base. Results: `0xCA01` bad size, `0xD001` too big for the region, `0xCE01` slab exhausted, `0xD401` bad state, `0x10801` resource limit. | DOC | Yes. Needed for a heap in B1 |
| S6 | same | `0x02` SetMemoryPermission: `X0` address, `X1` size, `W2` permission. Only none, R and RW are valid targets (no execute, no write-only). Results: `0xCC01` unaligned address, `0xCA01` zero or unaligned size, `0xD401` bad state, `0xD801` bad permission, `0xCE01` exhausted. | DOC | Yes |
| S7 | same | `0x03` SetMemoryAttribute: `X0` address, `X1` size, `W2` mask, `W3` value. Only the Uncached bit may be set or cleared (plus PermissionLocked on 17.0.0+). No result codes are listed. | DOC | Partly. B1 can accept it as a no-op on Normal memory (GUESS) |
| S8 | same | `0x07` ExitProcess: no arguments, no result, does not return. `0x0A` ExitThread likewise. | DOC | Yes |
| S9 | same | `0x27` OutputDebugString: `X0` = string pointer, `X1` = byte length; out `W0` result. No result codes are listed. The string is not NUL-terminated by contract (length is passed). | DOC | Yes. The "hello" of B1 |
| S10 | same | `0x26` Break: `X0` reason, `X1` unnamed, `X2` info. The page's table is inconsistent on whether it returns a result. If reason bit 31 is set the call just returns. | DOC, inconsistent | Partly. Log as a named stub in B1 |
| S11 | same | `0x80` SetGpuMemoryAttribute `[S2]`: same arguments as `0x03`. The same tag marks `0x81` LockGpuSharableMemory, `0x82` UnlockGpuSharableMemory, `0x84` MapRemoteMemory, `0x85` UnmapRemoteMemory and `0x87`/`0x88` (no names). `0x83`, `0x86` and `0x89`-`0x8F` are absent. The attribute bits 5 and 6 and four GetInfo ids (29-33) also carry `[S2]`. No result codes. | DOC; reading of `[S2]` is an inference | Yes. ADR-0024 reserves `0x80`-`0x8F` |
| S12 | docs/01-research-report.md E2 and docs/08 §3 | Emulator projects report a new SVC `0x80` on Switch 2 titles, for GPU memory attributes. This agrees with S11. | DOC (secondary) | Yes |
| S13 | web search, 2026-11-14 | No public source found documenting Switch 2 kernel SVCs beyond the wiki rows above. | DOC (absence) | Yes: treat Switch 2 as unknown |

## 4. Switch 1 knowledge versus Switch 2

- Switch 1 (DOC): every row above except S11-S13 describes Switch 1 firmware. Version tags such
  as "17.0.0+" and "23.0.0+" are Switch 1 firmware versions.
- Switch 2: nothing in this note is verified. Titles are reported to use a 39-bit address space
  and 6 cores (docs/01 E2), which does not contradict the layouts here. Whether QueryMemory type
  values, attribute bits and result codes are unchanged on Switch 2 is UNKNOWN.
- The meaning of `[S2]`, and whether `0x80` has the same register layout as `0x03` on Switch 2,
  is a GUESS from the wiki row alone.

## 5. Open questions

- O1: the NRO header version value and the exact alignment rule (AlignedHeader flag) are not
  given. B1 should write 0 and page-align segments (4 KiB, GUESS), then check against real
  behaviour only through public sources.
- O2: how the file start (a branch) relates to the module-header location, and what the MOD
  offsets are relative to (N8). Settle by re-reading the raw wiki pages. A static B1 NRO may
  place no MOD0 at all; whether the loader tolerates that is UNKNOWN.
- O3: the page-info output of QueryMemory (S2) and the QueryMemory/SetMemoryAttribute result
  codes are undocumented. B1 may return success and a zero page info (GUESS).
- O4: the full MemoryType table (S3) needs a raw re-read.
- O5: which Switch 2 SVC numbers other than `0x80` exist (S11 lists `0x81`-`0x88` with no
  Switch 2 confirmation).
- O6 (T-034 GUESSes, `crates/lince-kernel`): the heap base the kernel picks for SetHeapSize
  (B1 fixes `0x20_0000_0000`), shrinking the heap to sizes other than 0 or an earlier size,
  the OutputDebugString length limit (B1: 64 KiB), the extent of the Inaccessible region
  QueryMemory reports past the address space, and what Break does when reason bit 31 is clear
  (B1 stops the guest).

## 6. Affected ADRs, PoCs and tasks

- **ADR-0024:** the SVC table must reserve `0x80`-`0x8F`, with `0x80` named
  `SetGpuMemoryAttribute` and the unknown ones returning the distinct "unknown SVC" error. No ADR
  change needed otherwise.
- **T-029:** the ELF-to-NRO tool writes the `0x80`-byte header (N2-N4), zero flags, bss size
  and no asset block. Its header test checks these fields against this note.
- **T-032:** the loader maps text, ro, data and a zeroed bss, validates the magic and sizes,
  and rejects the Compress flag and sizes that overrun the file. It builds the config list (E2).
- **T-034:** SVCs `0x01`, `0x02`, `0x03`, `0x06`, `0x07`, `0x27`, plus `0x26` as a named stub.
- **docs/08 §4:** lists "Switch 2 SVC `0x80`–`0x88` semantics and result codes" and
  "QueryMemory type and attribute values on Switch 2" as UNKNOWN.
