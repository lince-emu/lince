# Research: B2 SVCs, CMIF/TIPC IPC and the sm:, set:, fs, time, hid commands (T-048)

- **Task:** T-048. **Date:** 2026-11-14.
- **Sources:** public Switchbrew wiki pages only (LEGAL-11). Wiki pages have no commit, so rows
  cite the URL and the access date. All facts were read through a page-summarising fetch tool,
  not the raw wiki text, and the tool sometimes said a page "does not cover" a field. Re-read the
  raw pages before T-051 to T-054 freeze their tests. Facts are restated in our own words; the
  wiki's licence was not checked, so do not paste its text or tables.
- Evidence levels: DOC (public doc, read), RECALL (from memory, unverified), GUESS.

## 1. Question

Which SVCs (number, registers, results), which CMIF and TIPC message layouts, and which `sm:`,
`set:`, `fs`, `time` and `hid` commands does the B2 program need? What is Switch 1 knowledge and
what is known on Switch 2?

## 2. Short answer

- The SVC numbers and registers for thread, TLS-adjacent, sync, event, handle and IPC calls are
  in section 3.1. The wiki lists result codes for only about six of them; the rest are UNKNOWN.
- A CMIF request is a HIPC header (two 32-bit words, optional special header with PID and
  handles, buffer descriptors), then 16-byte-aligned raw data that starts with a 16-byte
  `SFCI` header (method id, token). Domains add a 16-byte header and object ids.
- TIPC is a CMIF-less variant used only by `sm:` (and `pgl`) since 12.0.0. The wiki says the
  request id sits in the low 16 bits of the first header word and the payload is the raw data.
  The type values and result placement were NOT obtained (open question O1).
- A program reaches `sm:` by connecting to the named port `sm:` (SVC `0x1F`), registering itself,
  then asking `sm:` for a handle by service name (an 8-byte zero-padded name).
- All of it is Switch 1 knowledge. Switch 2 is known to change only a few `sm:` commands
  (ServiceSpace on cmds 2 and 3, 20.0.0+) and to remove `set:sys` GetFirmwareVersion.

## 3. Findings

### 3.1 SVCs (switchbrew.org/wiki/SVC, 2026-11-14, DOC unless noted)

| # | SVC (number): registers in -> out | Results listed | Applies? |
|---|---|---|---|
| V1 | CreateThread (`0x08`): X1 entry, X2 argument, X3 stack top, W4 priority, W5 core id -> W0 result, W1 handle | none | Yes (T-051) |
| V2 | StartThread (`0x09`): W0 handle -> result (page's two tables disagree on whether a result exists) | none | Yes |
| V3 | ExitThread (`0x0A`), no args, no return. SleepThread (`0x0B`): X0 nanoseconds (signed), no result | none | Yes |
| V4 | GetThreadPriority (`0x0C`): W1 handle -> W0, W1 priority. SetThreadPriority (`0x0D`): W0 handle, W1 priority | none | Yes |
| V5 | GetThreadCoreMask (`0x0E`): W2 handle -> W1 mask low, X2 mask high. SetThreadCoreMask (`0x0F`): W0 handle, W1 core id or -2 style value, X2 mask | none | Yes. The meaning of special core ids is not in the summary (O3) |
| V6 | GetCurrentProcessorNumber (`0x10`) -> W0 core id | none | Yes |
| V7 | SignalEvent (`0x11`), ClearEvent (`0x12`): W0 handle -> result. ResetSignal (`0x17`): W0 handle | `0xE401` invalid handle (V7,V8); `0xFA01` not signaled (ResetSignal) | Yes |
| V8 | CreateEvent (`0x45`) -> W1 writable handle, W2 readable handle | none | Yes |
| V9 | CloseHandle (`0x16`): W0 handle -> result | none (double close: UNKNOWN code, O4) | Yes |
| V10 | WaitSynchronization (`0x18`): X1 handle array, W2 count, X3 timeout (signed ns) -> W0 result, W1 index | `0x7601` termination, `0xE401` invalid handle, `0xE601` invalid address, `0xEA01` timeout, `0xEC01` cancelled, `0xEE01` too many handles | Yes (T-052) |
| V11 | CancelSynchronization (`0x19`): W0 thread handle | `0xE401` | Yes |
| V12 | ArbitrateLock (`0x1A`): W0 owner thread handle, X1 address, W2 tag. ArbitrateUnlock (`0x1B`): X0 address | none | Yes: this is the mutex |
| V13 | WaitProcessWideKeyAtomic (`0x1C`): X0 key address, X1 mutex address, W2 tag, X3 timeout. SignalProcessWideKey (`0x1D`): X0 key address, W1 count (signed) | none | Yes: this is the condition variable |
| V14 | GetSystemTick (`0x1E`) -> X0 counter | none | Yes (counter register vs SVC: see O5) |
| V15 | GetThreadId (`0x25`): W1 handle -> X1 id. GetProcessId (`0x24`) likewise | none | Optional |
| V16 | ConnectToNamedPort (`0x1F`): X1 name pointer -> W1 session handle | none | Yes (reach `sm:`) |
| V17 | SendSyncRequest (`0x21`): W0 session handle -> result. The request and reply travel in the thread's TLS message buffer | none | Yes |
| V18 | SendSyncRequestWithUserBuffer (`0x22`): X0 address, X1 size, W2 handle | `0xCC01` address unaligned, `0xCA01` size unaligned, `0xCE01` allocation or pointer-buffer overflow, `0xE401` bad handle | Later, not B2 |
| V19 | ReplyAndReceive (`0x43`), CreateSession (`0x40`), AcceptSession (`0x41`), ManageNamedPort (`0x71`), ConnectToPort (`0x72`) | `0xEA01` timeout, `0xF601` port remote dead (`0x43`); `0xF201` no session waiting (`0x41`) | Internal use by HLE services only (our servers are host code, not guest threads; GUESS) |
| V20 | GetInfo (`0x29`): W1 info type, W2 handle, X3 subtype -> X1 value | none | Maybe: programs read info ids at start (O6) |
| V21 | The wiki marks `0x34` WaitForAddress as 4.0.0+, so a modern libc may use it instead of V12/V13 | none | UNKNOWN which B2 uses (O7) |

Result codes are printed as 16-bit values; arithmetic shows `0xE401` = module 1, description 114
(INFERENCE from the encoding: low 9 bits module, rest description; verify on the Results page).

### 3.2 CMIF layout (switchbrew.org/wiki/IPC_Marshalling and /HIPC, 2026-11-14, DOC)

| # | Observation | Applies? |
|---|---|---|
| C1 | Header word 0: bits 0-15 message type, 16-19 pointer-descriptor count, 20-23 send-buffer count, 24-27 receive-buffer count, 28-31 exchange-buffer count. Word 1: bits 0-9 raw size in words, 10-13 receive-list count, 20-30 receive-list offset, 31 has-special-header. | Yes |
| C2 | Special header (one word, present when bit 31 set): bit 0 = PID follows (u64, 8 bytes), bits 1-4 copy-handle count, bits 5-8 move-handle count. Then the PID, copy handles, move handles (4 bytes each, RECALL for size). Handle 0 means none. | Yes |
| C3 | Message types: 0 invalid, 1 request, 2 close, 3 control, 4/5 old request/control (pre-5.0.0 layout), 6 request2, 7 control2 (5.0.0+). | Yes: B2 emits only 1/2/3/6/7 as needed |
| C4 | Pointer descriptor 8 bytes: word 0 = index (bits 0-3), address bits 36-38 (6-8), bits 32-35 (12-15), size (16-31); word 1 = address low 32. Map (send/receive/exchange) descriptor 12 bytes: size low, address low, then attribute bits 0-1, address high bits, size bits 32-35. Receive-list entry 8 bytes: address low, then address bits 32-38 and a 16-bit size. | Yes (T-053 round-trip) |
| C5 | Raw data is 16-byte aligned. Padding around the CMIF part is always 0x10 bytes in total. 16-bit sizes for out-pointer buffers follow the raw data. | Yes |
| C6 | CMIF in-header, 0x10 bytes: `SFCI`, version u16 (0, or 1 from 5.0.0), method id u32 at `+8`, token u32 at `+0xC` (5.0.0+). Out-header: `SFCO`, version, result u32 at `+8`, interface id at `+0xC` (14.0.0+). Parameters follow, sorted by alignment, mostly u64-sized. | Yes |
| C7 | Domain in-header (0x10): kind u8 (1 invoke, 2 close), object count u8, raw size u16, target object id u32, token at `+0xC`; CMIF message follows, then the in-object ids (u32 each). Out-header: object count u32 then 12 reserved bytes. | Yes, but B2 may skip domains (ADR-0024 decides, GUESS) |
| C8 | Control commands (message type 3 or 7): 0 ConvertCurrentObjectToDomain, 1 CopyFromCurrentDomain, 2 CloneCurrentObject, 3 QueryPointerBufferSize (u16 out), 4 CloneCurrentObjectEx. Close of a session is message type 2, not a numbered command. | Yes |
| C9 | Buffer attribute bits (HIPC map alias, pointer, fixed size, auto-select, non-secure, non-device) and native-handle attribute (copy, move). | Yes |

### 3.3 TIPC

| # | Observation | Level | Applies? |
|---|---|---|---|
| T1 | Simpler than CMIF; no domains, so one object per session. Used only by `sm:` (12.0.0+) and `pgl`. | DOC, HIPC page | Yes |
| T2 | Request id is in the low 16 bits of header word 0; the raw data holds the payload directly. | DOC | Yes |
| T3 | Command numbers on `sm:` change at 12.0.0; the summary said "reduced by 16", the direction is unclear. RECALL: the TIPC request type is 16 plus the command id, and the reply carries the result in its first raw word. | RECALL, unverified | Open (O1) |

### 3.4 Services (names are 8 characters, zero-padded, packed into a u64)

| # | Service and commands (switchbrew.org pages Services_API, Settings_services, Filesystem_services, Glue_services, HID_services, 2026-11-14) | Applies? |
|---|---|---|
| S1 | `sm:` interface (TIPC from 12.0.0): 0 RegisterClient (PID descriptor; before 12.0.0 also a reserved u64), 1 GetServiceHandle (name -> handle), 2 RegisterService, 3 UnregisterService, 4 DetachClient (11.0.0 only). On Switch 2, cmds 2 and 3 take a ServiceSpace (20.0.0+, tagged S2). Error codes (module 21) were not on the page. | Yes |
| S2 | `set` (ISettingsServer): 0 GetLanguageCode, 1 GetAvailableLanguageCodes (buffer), 2 MakeLanguageCode, 3 GetAvailableLanguageCodeCount, 4 GetRegionCode, 5 and 6 are the 4.0.0+ variants of 1 and 3. `set:sys` cmd 3 GetFirmwareVersion (0x100-byte out buffer) was removed with Switch 2. | Yes: B2 calls 0 and 4 (GUESS) |
| S3 | `fsp-srv` (there is no plain `fs` name): 1 SetCurrentProcess (u64 pid), 18 OpenSdCardFileSystem, 51 OpenSaveDataFileSystem (space u8 + 0x40-byte attribute). IFileSystem: 7 GetEntryType, 8 OpenFile (path buffer + mode bits: read 1, write 2, append 4), 9 OpenDirectory, 11 GetFreeSpaceSize. IFile: 0 Read, 1 Write, 2 Flush, 3 SetSize, 4 GetSize (parameters not obtained). IDirectory: 0 Read, 1 GetEntryCount. | Yes: B2 needs 1, 18, 8, IFile 0/4 (GUESS) |
| S4 | `time:u` (also `time:a`, `time:r`): 0 GetStandardUserSystemClock, 1 GetStandardNetworkSystemClock, 2 GetStandardSteadyClock, 3 GetTimeZoneService, 4 GetStandardLocalSystemClock. ISystemClock 0 GetCurrentTime (s64 POSIX). ISteadyClock 0 GetCurrentTimePoint (0x18 bytes: u64 + 16-byte clock id). ITimeZoneService 0 GetDeviceLocationName (0x24 bytes), 101 ToCalendarTimeWithMyRule. | Yes: B2 uses 0 then ISystemClock 0 |
| S5 | `hid`: 0 CreateAppletResource (PID, u64 resource user id -> IAppletResource), 11 ActivateTouchScreen, 100 SetSupportedNpadStyleSet, 102 SetSupportedNpadIdType (buffer of at most 10), 103 ActivateNpad, 123 SetNpadJoyAssignmentModeSingle. IAppletResource 0 GetSharedMemoryHandle: parameters not obtained. None of these is tagged S2. | Yes: B2 stub returns success and logs |
| S6 | Some services are S2-only (`adraw:a`, `chat:a`, `fs:stcfg`) and some S1-only (`btdrv`); the service list has separate columns. `set`, `fsp-srv`, `hid` and `time:*` exist on both. | Yes |

## 4. Switch 1 versus Switch 2

- Everything in 3.1 to 3.3 is Switch 1 firmware knowledge. No page confirms SVC numbers, result
  codes or message layouts on Switch 2. The only S2 tags seen are in S1 and S2 above.
- UNKNOWN on Switch 2 (to add to doc 08 §4): thread and sync SVC registers and results; whether
  CMIF/TIPC headers are unchanged; the ServiceSpace values; the `hid`, `time` and `fsp-srv`
  command ids beyond the tagged ones.

## 5. Open questions

- O1: TIPC message type values, result placement, and the 16 offset (T3). Re-read the raw page
  and the sm: page. Until then T-053 builds `sm:` behind a layout that is easy to change.
- O2: IFile Read/Write parameters and the IAppletResource shared-memory handle command.
- O3: SetThreadCoreMask special core ids, and GetInfo ids B2 reads.
- O4: result codes for CloseHandle (bad handle, double close), thread SVCs, ArbitrateLock and
  WaitProcessWideKey; the wiki lists none. Needed by T-052's "documented error".
- O5: whether the tick comes from `GetSystemTick` or the counter register (T-050).
- O6: which `sm:` error values (module 21) an unknown service name returns.
- O7: whether B2's guest runtime uses WaitForAddress. B2 test code is team-written, so we choose.

## 6. Affected ADRs, PoCs and tasks

- **ADR-0024:** SVC table gains the rows in 3.1; domains and TIPC need an explicit in or out for
  B2 (C7, T1).
- **T-049:** program uses only calls in S1 to S5 that this note documents; mark GUESS items.
- **T-051, T-052, T-053, T-054:** use the tables above; O4 and O6 get GUESS result codes marked
  as such.
- **docs/08 §4:** lists the UNKNOWN items from section 4.
