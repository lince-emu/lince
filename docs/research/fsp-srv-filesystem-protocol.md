# Research: fsp-srv IFileSystem, IFile and IDirectory parameters (T-062)

- **Task:** T-062. **Date:** 2026-11-15. Behavior only; no code copied.

## 1. Question

For a file system served from a host directory (T-064): what are the parameters and buffers of
IFile Read, Write, GetSize, SetSize (open item O2 of `b2-svcs-and-ipc.md`); the HIPC descriptor
kind and size of the IFileSystem path buffer; how a missing path is reported; and the entry
layout of IDirectory Read. Switch 1 versus Switch 2.

## 2. Short answer

- Paths are a fixed 0x301-byte (769) NUL-terminated string, sent as a HIPC pointer-kind input
  buffer (X descriptor), not a map-alias buffer. Switchbrew tags it 0x19 for OpenFile and 0x9
  for GetEntryType/OpenDirectory (the 0x10 bit means fixed size); libnx sends all three the same
  way, so treat the kind as one for all.
- IFile Read (cmd 0): raw input u32 option, u32 pad, s64 offset, u64 size; data goes out through
  a map-alias output buffer; reply is a u64 byte count. Write (cmd 1): same 24-byte input, data
  in a map-alias input buffer, no reply data. GetSize (4): s64 out. SetSize (3): s64 in.
- Missing path: the result name is 2-1 `PathNotFound` (0x202). Public sources give the code but
  not which command returns it in which case; that part stays GUESS.
- IDirectory Read (cmd 0): map-alias output buffer of entries, reply u64 entry count; entry is
  0x310 bytes (name 0x301, attributes/padding, type s8 at 0x304, size s64 at 0x308).
- Switch 2: no public source shows any change to these commands; assume unchanged (GUESS).

## 3. Findings

Pins: libnx `switchbrew/libnx` branch master, commit b156b4774fb2df7b93402169509266da7547fc4
(HEAD on 2026-11-15, files `nx/source/services/fs.c`, `nx/include/switch/services/fs.h`).
Switchbrew pages are wiki revisions read on 2026-11-15 (no revision id captured).
libnx is ISC-licensed public homebrew code; only behavior was extracted.

| Source | Observation | Evidence | Applies to Lince? |
|---|---|---|---|
| switchbrew.org/wiki/Filesystem_services | fsp-srv cmd 0 OpenFileSystem (type-0x19 path buffer + type u32), removed in 2.0.0; cmd 18 OpenSdCardFileSystem has no parameters or output listed | DOC | Yes: cmd 18 only (S3 in b2 note) |
| libnx fs.c@b156b47 | OpenSdCardFileSystem: cmd 18, no input, returns one object (IFileSystem) | SRC | Yes |
| switchbrew Filesystem_services | IFileSystem GetEntryType cmd 7: path buffer type 0x9, out u32 DirectoryEntryType | DOC | Yes |
| libnx fs.c@b156b47 | GetEntryType: path as pointer-kind input buffer of FS_MAX_PATH; output is the entry type value | SRC | Yes |
| switchbrew Filesystem_services | OpenFile cmd 8: path buffer type 0x19 + u32 mode; bit0 read, bit1 write, bit2 append | DOC | Yes |
| libnx fs.c, fs.h@b156b47 | OpenFile: mode u32, path pointer-kind in-buffer FS_MAX_PATH, returns one object. FS_MAX_PATH = 0x301 | SRC | Yes |
| switchbrew Filesystem_services | Without the append bit, a write past end of file fails with 0x307202 (module 2, description 0x3839) | DOC | Yes: no auto-grow without bit 2 |
| switchbrew Filesystem_services | OpenDirectory cmd 9: path buffer 0x9 + u64 filter_flags (bit0 dirs, bit1 files) | DOC | Yes |
| libnx fs.h@b156b47 | Directory open flags: 1 dirs, 2 files, 0x80000000 "no file size" (entries report size 0) | SRC | Yes (honor 3 flags) |
| libnx fs.c@b156b47 | Open mode flags: read 1, write 2, append 4 | SRC | Yes |
| libnx fs.c@b156b47 | IFile Read cmd 0: input u32 option, u32 pad, s64 offset, u64 size (24 bytes); out buffer is map-alias of `size` bytes (non-secure allowed); reply u64 bytes read | SRC | Yes |
| libnx fs.h@b156b47 | Read option: only 0 (None) defined | SRC | Yes: ignore option |
| libnx fs.c@b156b47 | IFile Write cmd 1: same 24-byte input, map-alias input buffer of `size` bytes, no reply data | SRC | Yes |
| libnx fs.h@b156b47 | Write option: 0 none, bit0 = flush after write | SRC | Yes: bit0 means flush on host |
| libnx fs.c@b156b47 | IFile Flush cmd 2 (no data), SetSize cmd 3 (s64 in), GetSize cmd 4 (s64 out) | SRC | Yes |
| switchbrew Filesystem_services | IFile command list: 0 Read, 1 Write, 2 Flush, 3 SetSize, 4 GetSize, 5 OperateRange (4.0.0+), 6 OperateRangeWithBuffer (12.0.0+); no parameters shown on that page for IFile | DOC | Yes: ids only; 5 and 6 not needed |
| switchbrew Filesystem_services | IStorage Read takes type-0x46 buffer, offset, length; Write type-0x45 (IStorage, not IFile) | DOC | Partly: 0x45/0x46 are map-alias in/out; IFile uses the same kind per libnx (inference) |
| switchbrew Filesystem_services | IDirectory Read cmd 0: type-0x6 output buffer of DirectoryEntry array; reply u64 count, 0 when none remain; "." and ".." never listed. GetEntryCount cmd 1 returns u64 | DOC | Yes |
| libnx fs.c@b156b47 | Dir Read: map-alias out buffer of max_entries x entry size; reply count; GetEntryCount cmd 1 | SRC | Yes |
| switchbrew Filesystem_services | DirectoryEntry: 0x0 path[0x301]; 0x301 attributes (bit0 dir, bit1 archive); 0x304 type s8; 0x308 size u64 (0 for dirs); total size not stated | DOC | Yes |
| libnx fs.h@b156b47 | FsDirectoryEntry: name[0x301], 3 pad bytes, type s8, 3 pad bytes, s64 file_size. Natural alignment gives size 0x310 (computed, not stated) | SRC | Yes |
| libnx fs.h, switchbrew | Entry type values: 0 directory, 1 file | SRC, DOC | Yes |
| switchbrew.org/wiki/Error_codes | Module 2: 2-1 (0x202) PathNotFound, 2-2 (0x402) PathAlreadyExists, 2-7 (0xE02) TargetLocked; names only | DOC | Yes |
| switchbrew Filesystem_services | Page states no missing-path result for GetEntryType/OpenFile/OpenDirectory | DOC (absence) | n/a |
| switchbrew Filesystem_services | Switch 2 marks on the page ([S2]): content attribute value, LocalKeySeedPackage size, storage-device manager objects; nothing on IFileSystem/IFile/IDirectory | DOC (absence) | Yes: no S2 change known |

Conflicts and corrections:
- Switchbrew uses buffer type 0x9 for some commands and 0x19 for others; libnx uses one kind.
  Take pointer-kind with 0x301 size for all (inference; the 0x10 bit meaning is RECALL).
- libnx names the Dir Read reply a "total entry count"; Switchbrew says number of entries read
  into the buffer (0 at the end). Use Switchbrew's meaning (entries written this call).
- Switchbrew's DirectoryEntry has "attributes" at 0x301; libnx calls the bytes padding. Lince
  can write 0 there or the dir/archive bits (GUESS: guests ignore it).

## 4. Open questions (go to GUESS for T-064 and doc 08)

- GUESS: GetEntryType, OpenFile (without write/create bit) and OpenDirectory on a missing path
  return 2-1 PathNotFound (0x202); OpenFile on a directory and OpenDirectory on a file also
  report PathNotFound (inference from the single name; no source).
- GUESS: the reply byte count of Read is short at end of file and 0 past it, with success.
- UNKNOWN: reply when the path exceeds 0x301 or lacks NUL; Read with offset past EOF; the
  result of OpenFile with append bit on a file opened read only; write-share conflicts.
- UNKNOWN: whether CreateFile (cmd 0) and friends are needed by B2 programs; their parameters
  (size, option) were not extracted.
- UNKNOWN: any Switch 2 change (fsp-srv version gating, new path rules). Only the Switchbrew
  [S2] tags above were read; add a line to doc 08 saying fs commands are assumed unchanged.
- Path text: the "@Sdcard:/" style prefix and leading "/" handling for the SD-card file system
  were not researched; B2 only needs paths relative to the SD root (GUESS).
- Full Switchbrew page text was read through 100000 of 124066 characters via a summarizer;
  the final part and the exact revision were not checked.

## 5. Affected

- `docs/research/b2-svcs-and-ipc.md` S3 and O2: parameters now filled for IFile 0, 1, 3, 4 and
  IDirectory 0, 1.
- ADR-0024 (B2 scope, services): no change; the fixed-size path is a pointer-kind buffer, so
  the HIPC framework (T-053) must support in-pointer descriptors with a fixed length.
- T-064 (`fsp-srv` on a host directory): implement from this note; keep the GUESS items as
  `// GUESS:` comments. T-063 unaffected.
- Doc 08: add the UNKNOWN list above.
