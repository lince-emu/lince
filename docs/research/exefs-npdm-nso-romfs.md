<!-- SPDX-License-Identifier: CC0-1.0 -->
# T-070: ExeFS, NPDM, NSO, the rtld/subsdk module chain and RomFS

Date: 2026-11-16. Task: T-070. Sources are public wikis and public project docs only. No code was
copied. No keys, firmware or leaked material were used. Nothing here was run against a real title.

## Question

What do the team-written loader (T-073), the test title (T-072) and the RomFS service (T-075) need
to know about: the ExeFS container, the NPDM metadata file, the NSO executable format, the order
and placement of modules (rtld, main, subsdk, sdk), the MOD0 header, and the RomFS image?

## Short answer

- ExeFS is a PFS0 holding `main.npdm` plus up to `rtld`, `main`, `subsdk0..9`, `sdk` (NSO files).
  Modules are loaded back to back in the order rtld, main, subsdk0..9, sdk. The first module
  loaded is the entry point.
- NPDM is META (0x80 bytes) + ACID + ACI0. It gives the address-space type, main-thread
  priority/core/stack size, system-resource size, filesystem/service/kernel permissions.
- NSO is a 0x100-byte header (segments, build id, compressed sizes, SHA-256 per segment, flags)
  followed by compressed text, rodata and data. `.bss` is a size only.
- rtld finds the other modules by scanning memory for R-X STATIC regions, and each module carries a
  MOD0 header pointed to from `.text+4`. The loader does not link modules; rtld does.
- RomFS: 0x50-byte header, two hash tables, two entry tables, data. Title RomFS is wrapped by IVFC
  hashing in the NCA, which Lince does not need (T-075 serves an extracted directory).

## Findings

Evidence: SRC (source at pinned commit), DOC (project doc), MIRROR (derived from a second
public source), RECALL (memory, to verify), INFER (our inference).

| # | Source | Observation | Ev. | Lince? |
|---|---|---|---|---|
| 1 | switchbrew.org/wiki/NCA (2026-11-16) | ExeFS section is a PFS0. Header: magic, entry count (+4), string table size (+8), reserved (+C). Entry table at +0x10, 0x18 bytes each (u64 offset, u64 size, u32 name offset, u32 reserved), then strings, then data. | DOC | Yes. T-072 writes it, T-073 reads it |
| 2 | switchbrew.org/wiki/ExeFS | `main` and `main.npdm` always exist. `rtld`, `sdk` and `subsdk*` are optional. Page lists subsdk0-6 only. | DOC | Yes. Accept 0-9 (row 3) |
| 3 | Atmosphere-NX/Atmosphere@3e89150 `docs/components/modules/loader.md` | Search order is `rtld`, `main`, `subsdk0`..`subsdk9`, `sdk`. Each NSO found is loaded after the previous one. Entry point is the first NSO loaded. `main.npdm` is read for permissions. | DOC | Yes. This is the order T-073 needs |
| 4 | switchbrew.org/wiki/Memory_layout | ExeFS binaries sit back to back in the main-binary region, same order. Within a binary: R-X, then R-- directly after, then RW- directly after, no guard pages. Base is chosen by Loader and passed to the kernel at process creation. Randomized on real hardware. | DOC | Yes. Lince may fix a base (ADR-0019/0024 decide) |
| 5 | switchbrew.org/wiki/NSO | Header: magic `NSO0` (0x0), version, flags (0xC). Three segment headers at 0x10/0x20/0x30: file offset, memory offset, size (u32 each). Module-name field at 0x1C and 0x2C. `.bss` size at 0x3C. Build id 0x20 bytes at 0x40. Compressed sizes at 0x60/0x64/0x68. Embedded/DynStr/DynSym extents at 0x88-0x9C (relative to rodata). SHA-256 of the three decompressed segments at 0xA0/0xC0/0xE0. Segment data from 0x100. | DOC | Yes. Layout for loader and test builder |
| 6 | same | Flags: bits 0-2 mean each of text/rodata/data is compressed. Bits 3-5 mean verify that segment's hash on load. Bit 6 execute-only (20.0.0+). Bit 7 selects a zstd-based codec instead of LZ4 (22.0.0+). | DOC | Partly. Support the compressed/hash bits. Reject bits 6-7 until a title needs them |
| 7 | NSO page + RECALL | The pre-22 codec is LZ4 block format (no frame header). The wiki text fetched did not name it. | RECALL | Verify with a team-made sample (T-072) |
| 8 | switchbrew.org/wiki/Rtld | rtld is the crt0. Entry is start of `.text`. On entry X0 is null and X1 is the main thread handle. rtld relocates the NSOs the loader mapped. | DOC | Yes. HLE must set X0=0, X1=thread handle |
| 9 | same | Each module has `MOD0` magic, reached through a pointer at `.text+4` (offset relative to that module's text). The header points to `.dynamic` and to `.bss` start/end. If the bss pointers differ, rtld zeroes it. | DOC | Yes. Test NSOs need a valid MOD0 |
| 10 | same | rtld finds modules with repeated `svcQueryMemory` from its own address: a candidate is a region that is R-X and STATIC; next query is address + region size. | DOC | Yes. Needs QueryMemory to report type and permissions per NSO segment (B1 SVC, `nro-and-b1-svcs.md`) |
| 11 | same | rtld applies only RELATIVE relocations to itself and modules, then resolves ABS64, ABS32, GLOB_DAT, JUMP_SLOT using dynsym, dynstr and `.hash`. Then it initializes the libc and SDK, runs module init, `main`, SDK fini, `svcExitProcess`. | DOC | Only if the test title uses a real rtld. T-072 uses a loader-stub NSO |
| 12 | switchbrew.org/wiki/NPDM | META: magic `META`, Flags0 (0xC: bit 0 is 64-bit, bits 1-3 address-space type, bit 4 optimize allocation), main thread priority (0xE, 0..0x3F), core (0xF), system resource size (0x14, max 0x1FE00000), version (0x18), stack size (0x1C, 0x1000 aligned), name (0x20, 0x10 bytes), ACI offset/size (0x70/0x74), ACID offset/size (0x78/0x7C). | DOC | Yes |
| 13 | same | Address-space values: 0 = 32-bit, 1 = 64-bit/36-bit (earlier "64BitOld"), 2 = 32-bit no reserved, 3 = 64-bit/39-bit, 4 = 64-bit/42-bit (names are from 23.0.0+ docs). | DOC | Yes. Games are almost all 3. Accept 3 first, reject others with a coverage-report entry |
| 14 | same | ACI0: magic, program id (0x10), then offset/size pairs for FS access (0x20), service access (0x28), kernel capabilities (0x30). ACID has a 0x100-byte signature and 0x100-byte key before its `ACID` body; a body offset is relative to the signature start. | DOC | ACI0 yes. Ignore signature contents (no verification, ADR-0029) |
| 15 | same | Service access list: one entry per service. Byte 0 low 3 bits are name length minus 1. Bit 7 means "may register". Name follows. `*` wildcard allowed. | DOC | Yes. Enforcing it is optional; log violations |
| 16 | same | Kernel capabilities: 32-bit descriptors, type given by the trailing-ones pattern. Thread info (priority and core ranges), enabled SVC bitmask (index in bits 29-31, 24 mask bits), memory maps, IO maps, interrupts, program type, kernel version, handle table size, misc flags. | DOC | Subset. SVC mask and handle table size matter for B3 |
| 17 | Atmosphere-NX/Atmosphere@3e89150 loader.md | NSOs may be replaced or blocked per file by the user's side files. Patches (IPS/IPS32) are named after the build id. | DOC | No. Out of scope (patching is a separate decision) |
| 18 | Atmosphere-NX/Atmosphere@3e89150 `stratosphere/ams_mitm/source/fs_mitm/fsmitm_romfs.cpp` (behavior only) | Switch RomFS header is 0x50 bytes. Entries are linked lists (parent, sibling, child dir, child file, next-in-bucket, name length, name padded to 4). Directory entry fixed part 0x18, file entry 0x20 (file adds 64-bit data offset and size). Empty link = `0xFFFFFFFF`. Root dir is at table offset 0 with empty name. | SRC | Yes for T-075 and T-072 |
| 19 | same | Name hash: seed = parent entry offset XOR a constant (123456789); per byte: rotate right 5, XOR byte. Bucket = hash mod bucket count. Bucket count rule: <3 entries gives 3; 3-18 gives odd; >=19 grows until not divisible by 2,3,5,7,11,13,17. | SRC | Needed only to write images (T-072). A reader may walk the tree and ignore buckets |
| 20 | same | Image layout the builder produces: file data from 0x200, then dir hash table, dir table, file hash table, file table, each 4-aligned. File data aligned to 0x10. | SRC | Yes for T-072 |
| 21 | switchbrew.org/wiki/NCA | RomFS in an NCA is wrapped in IVFC (magic `IVFC`, up to 6 hash levels, 0x20 master hash). | DOC | No. Lince serves extracted directories, so no IVFC or hashing |
| 22 | INFER from #18 | Header field order is likely: header size (u64), then offset+size (u64 each) for dir hash, dir table, file hash, file table, then file data offset (u64). This adds up to 0x50. Not seen written out in a fetched source. | INFER | Confirm in T-072 against libstratosphere RomFS code at the same commit |

## Open questions

1. UNKNOWN: exact MOD0 field layout (magic, offsets to dynamic, bss start/end, eh_frame_hdr,
   module object). The Rtld page confirms only the pointer at `.text+4`, `.dynamic` and bss.
   Read it from libnx/Atmosphere docs at a pinned commit before T-072.
2. GUESS (finding 7): the pre-22.0.0 codec is LZ4 block format. UNKNOWN: whether `.bss`
   always follows `.data` page-aligned.
3. UNKNOWN: the first module's entry registers when `rtld` is missing from ExeFS (some titles
   ship `main` only).
4. GUESS: each segment starts page-aligned (0x1000), from the memory-layout page; not stated.
5. GUESS: a fixed load base is acceptable to titles, since rtld uses relative addressing.
6. Yuzu mirror sources are DMCA-blocked on GitHub. They were not used.

## Switch 2

No fetched public source describes Switch 2 ExeFS, NPDM, NSO or RomFS. Known: tested titles
are 64-bit with the 39-bit address space (type 3) and core mask `0x3f` (doc 01 E2). UNKNOWN:
whether the NSO header, flags and codec are unchanged; in particular whether Switch 2 titles
use the zstd-based codec of flag bit 7 (finding 6), which would need a second decompressor;
whether the NPDM META/ACI0 layout and capability descriptors are unchanged; whether new SVCs
(`0x80`–`0x88`) appear in the capability SVC mask the same way. T-073 rejects what it does not
know with a typed error, and T-081 on a real title answers these. Listed in doc 08 §4.

## Affected ADRs, PoCs and tasks

- ADR-0024 (loader/HLE scope) and ADR-0025: confirm the module order and the "no IVFC" choice.
- ADR-0019 (address space): fixed base for the first module, and the 39-bit type first.
- T-072 (test title builder): can build PFS0, NPDM (META + ACI0 + minimal ACID), NSO with MOD0
  and a RomFS using findings 1, 5, 9, 12, 18-20.
- T-073 (`lince-loader`): findings 3-6, 12-16.
- T-075 (RomFS from a directory): findings 18-21. Needs only directory walk, file size, offset.
- B1 SVC work: QueryMemory must expose R-X STATIC for each module (finding 10).
