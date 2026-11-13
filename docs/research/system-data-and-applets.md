# System data archives and library applets used by Switch 2 titles (firmware-free)

Status: Research note for T-019 · Last reviewed: 2026-11-13 · Rules: ADR-0029, LEGAL-05/06/10/11/50–52

Sources used (all public; no firmware, keys, saves, game files or SDK material were opened):

| Short name | Location | Pin |
|---|---|---|
| Lichii | github.com/zortos293/Lichii, branch `nx2` | commit `0da698bb98c619e7eaec7dd2db80173c5134c255` (2026-11-13) |
| Eglantine | github.com/Eglantine-Community/Eglantine, branch `nx2` | commit `66ce46feb67a184ee4adae59ed07cf89c72aac9c` (2026-11-12) |
| SB-S2TL | https://switchbrew.org/wiki/Switch_2:_Title_list | accessed 2026-11-13 |
| SB-AM | https://switchbrew.org/wiki/Applet_Manager_services | accessed 2026-11-13 |
| SB-PL | https://switchbrew.org/wiki/Shared_Database_services | accessed 2026-11-13 |
| SB-SWKBD | https://switchbrew.org/wiki/Software_Keyboard | accessed 2026-11-13 |
| SB-CTRL | https://switchbrew.org/wiki/Controller_Applet | accessed 2026-11-13 |
| SB-ERRAPP | https://switchbrew.org/wiki/Error_Applet | accessed 2026-11-13 |
| SB-ERR | https://switchbrew.org/wiki/Error_codes | accessed 2026-11-13 |

Lichii source was read through the GitHub contents API at the pinned commit (no clone). Only
behavior is recorded here; no code was copied (LEGAL-20/21).

## 1. Question

Which system data archives and which library applets do Switch 2 titles depend on, how does a
game reach each one, and for each, what should Lince do: (a) an original implementation,
(b) a freely licensed substitute, or (c) a documented "unavailable" behavior?

## 2. Short answer

Most system data reaches games through services Lince implements (`pl:u` fonts, `time:*`,
`ngc:u`, `mii:*`); a few titles mount archives with fsp-srv cmd 202. Lince needs OFL-1.1 fonts
and a tzdata build (**S**), and its own glyphs, empty profanity filter and version profile
(**O**); every other archive is system-only or online-only (**U**). Keyboard, error, profile
selector, controller and parental-PIN applets get original HLE UIs; the others return Canceled.
Switch 2-only Mii, camera and GameChat data stay **Unknown** and unavailable.

## 3. Findings

| # | Source | Observation | Evidence | Applies to Lince? |
|---|---|---|---|---|
| F1 | SB-S2TL, "System Data Archives" | Switch 2 system data uses the `04…` prefix with the same low IDs as Switch 1. Archives marked "Same as NX" include CertStore (…0800), MiiModel (…0802), NgWord (…0806), NgWord2 (…0823), TimeZoneBinary (…080E) and the five font archives (…0810–…0814). In FontNintendoExtension (…0810), `nintendo_ext2_003.bfttf` was removed. SystemVersion (…0809) no longer has its digest file (20.1.1+). | DOC | Yes. The Switch 1 formats are a valid model for these archives. |
| F2 | SB-S2TL | Archives found only on Switch 2: …0844 "data for face detection", …0847 "MiiBody.dat and shaders", …0849 "appears to be MiiModel updated for Ounce", …0860 CameraFirmware, …081E JetShared (controller icon footer), …0848 ApplicationCompatibilityInfo, …0828 FunctionBlackList. The fetched table pairs some names and IDs ambiguously. | DOC (row attribution partly unclear) | Partially. These are listed as Unknown in §4. |
| F3 | Lichii `src/core/file_sys/system_archive/system_archive.cpp` (also Eglantine, same file) | When the user has no firmware, the fork builds the following archives in memory: MiiModel, NgWord, NgWord2, SystemVersion, TimeZoneBinary and the font archives. All other IDs it knows return nothing. A comment names a Switch 2 title that aborts when Switch 2 data …0849 cannot be mounted. For any `04…` data ID, the fork therefore serves an **empty RomFS** by default; an environment variable turns this off. | SRC | Yes for the behavior. Lince keeps an explicit list of IDs instead of covering every `04…` ID. |
| F4 | Lichii `src/core/hle/service/filesystem/fsp/fsp_srv.cpp` (`OpenDataStorageByDataId`) | Before building an archive, the fork first tries content installed by the user (firmware). If both fail, it returns a placeholder "unknown" result, and a TODO says the real error code is not known. | SRC | Lince never takes the firmware path (ADR-0029). The correct "not found" result is an open question. |
| F5 | Lichii `NX2_NOTES.md` row 36 | Switch 2 titles read fonts through `pl:u` 2000 (font shared memory plus its size) and 2010 (`GetBuiltInFontData`: a "loaded" bool, then entries of flags, u32 offset and u32 size). The fork puts plain TTF copies after the obfuscated BFTTF copies used on Switch 1. | DOC | Yes. Lince serves fonts through the Switch 2 path; the Switch 1 path is pending decision D-009. |
| F6 | SB-PL | `pl:s`/`pl:u` font commands: RequestLoad, GetLoadState, GetSize, GetSharedMemoryAddressOffset, GetSharedMemoryNativeHandle (mapped read-only, **0x1100000 bytes**) and GetSharedFontInOrderOfPriority. Six font types: Standard, ChineseSimplified, ExtChineseSimplified, ChineseTraditional, Korean, NintendoExtended (button and icon glyphs). | DOC | Yes. All substitute fonts must fit in 0x1100000 bytes together. |
| F7 | Lichii `system_archive/shared_font.cpp`, `system_archive/data/*`, `externals/nx_tzdb/CMakeLists.txt` | The fork's built-in fonts are byte arrays with no recorded upstream font or font license. Its time-zone archive is built from a prebuilt download produced by the third-party `tzdb_to_nx` tool. | SRC | Do not reuse either. The font provenance is unknown. Lince builds its own time-zone data from IANA tzdata. |
| F8 | Lichii `system_archive/ng_word.cpp`, `src/core/hle/service/ngc/ngc.cpp` | The NgWord archives are built as small, valid files holding one placeholder pattern and a version file. `ngc:u` Check reports no match and Mask/Filter return the input unchanged. | SRC | Yes for the behavior: an empty, permissive filter, written by Lince. |
| F9 | Lichii `system_archive/mii_model.cpp` | The MiiModel archive contains the six expected file names. Each file holds only a 16-byte header (format magic and a version) and no model data. | SRC | Partially. This is enough to mount but not to render. See §4. |
| F10 | Lichii `NX2_NOTES.md` rows 123–127 | After a 1.8.0 update, a Switch 2 title calls GameChat initialization at boot (the notes quote an SDK symbol name; Lince does not adopt it, LEGAL-12) and blocks forever unless `chat:u` is registered. It also creates a camera context through gsv 601/602. The fork returns stub sessions, and the title then plays. | DOC | Yes. GameChat and camera must report unavailable without hanging. The exact result is Unknown. |
| F11 | SB-AM | AppletId list (Switch 1 values 0x0A–0x37, 0x64 splay). On Switch 2, AppletIds carry a variant in the upper bytes: 0x0100000E Error, 0x01010011 SwkbdModeless, 0x01020011 Swkbd, several MiiEdit variants (…0012), 0x01000013 OpenWeb, 0x01000015 Album, 0x01000017 Offline, 0x01000018 LoginShare (Lobby), 0x01000024/0x01030024 Splay, 0x01000028 Eula, 0x01000065–0x01030065 Chat. LibraryAppletExitReason: 0 Normal, 1 Canceled, 2 Abnormal, 10 Unexpected. `OpenApplicationProxy2` (cmd 10) and IChatProxy are Switch 2 only. | DOC | Yes. The applet dispatcher must decode Switch 2 AppletIds. |
| F12 | SB-S2TL, "System Applets" | Switch 2 applet programs include Controller, Account, TextEditor, NxSwkbd (20.1.0+), MiiEdit, Album, LibAppletOff (offline web), MyPage, SystemWeb/openweb and others. | DOC | Yes, for the inventory. Program IDs do not matter: Lince never loads applet programs. |
| F13 | Lichii `src/core/hle/service/am/frontend/applets.cpp`, `am_types.h`, `applet_general.cpp` | Applets with their own frontend: Auth, Cabinet, Controller, Error, ProfileSelect, SoftwareKeyboard, MiiEdit, the web family (Web, Shop, OfflineWeb, LoginShare, WebAuth, Lhub share one), PhotoViewer and NetConnect. Any other ID falls to a stub that pushes a 0x1000-byte zeroed output and interactive storage, then exits normally. The fork's enum holds only Switch 1 values. | SRC | Partially. Zeroed output silently looks like success to some applets (swkbd reads result 0 as "OK"). ADR-0024 forbids this, so Lince returns an explicit cancel or unavailable result instead. |
| F14 | SB-SWKBD | Normal swkbd output: a 0x7D8-byte storage. The u32 at offset 0 is 0 for OK and 1 for Cancel; UTF-16 text follows at offset 4. The KeyboardConfig size depends on the applet version (0x3E0, 0x400 or 0x4C8 bytes). The inline keyboard replies with DecidedEnter (5) or DecidedCancel (6). The page does not mention Switch 2. | DOC | Yes. This is the output contract for Lince's HLE keyboard. |
| F15 | SB-CTRL | Controller applet output: 0xC bytes holding playerCount (s8), selectedId (u32 NpadId) and a u32 result value. 0 maps to Result 0, 2 to 0x183CCA, and any other value to 0x183ACA. | DOC | Yes. |
| F16 | SB-ERRAPP | Error applet ErrorTypes 0–8 (SystemData, SystemError, ApplicationError, Eula, ParentalControl, …). Output is optional. The ErrorMessage archive is not mentioned. | DOC | Yes. Lince's HLE dialog shows the error code without firmware message text. |
| F17 | SB-ERR, module 128 and module 144 | am 0x4680 means "Error while launching applet". Module 144: 0x690 is Canceled and 0x290 is Exited Abnormally. | DOC | Yes. These are the results Lince returns from `GetResult` for unavailable applets (§4). |
| F18 | Lichii `NX2_NOTES.md` "Where it stops now" (title 4: "difficulty select, keyboard input") together with `EDEN_NX2_KEY_SCRIPT` | "Keyboard input" here means host keys scripted into the game, not the software keyboard applet. | DOC | **Correction:** doc 10 §3 cited this as evidence for the software keyboard applet before T-019; this change corrects it. |

## 4. Inventory

Lince approach codes: **O** = original implementation; **S** = freely licensed substitute;
**U** = unavailable, with the documented return behavior. "Svc" means the game reaches the data
only through a Lince service, so the archive file format matters only if a game mounts the
archive itself.

### 4.1 System data archives

| Archive (Switch 2 ID per SB-S2TL) | What games use it for | How the game reaches it | Lince approach | Evidence |
|---|---|---|---|---|
| FontStandard (…0811) | Latin and Japanese UI text | Svc `pl:u` (Switch 2 2000/2010; Switch 1 commands 0–5 pending D-009) | **S**: BIZ UDPGothic (Morisawa, SIL OFL-1.1). Fallback: Noto Sans JP (OFL-1.1). Metric compatibility is UNKNOWN until measured (open question 1). | DOC (F5, F6) |
| FontChineseSimple / ExtChineseSimplified (…0814) | Simplified Chinese text | Svc `pl:u` | **S**: Noto Sans SC (OFL-1.1), TrueType outlines, subset to fit the shared memory | DOC (F6) |
| FontChineseTraditional (…0813) | Traditional Chinese text | Svc `pl:u` | **S**: Noto Sans TC (OFL-1.1), subset | DOC (F6) |
| FontKorean (…0812) | Hangul text | Svc `pl:u` | **S**: Noto Sans KR (OFL-1.1), subset | DOC (F6) |
| FontNintendoExtension (…0810) | Button, stick and controller icons in the private-use area | Svc `pl:u` | **O**: glyphs drawn by Lince at the same codepoints (codepoint list still to be researched). Glyphs that would depict Nintendo logos, marks or hardware trade dress are drawn as neutral generic shapes or left blank (LEGAL-40; D-009). Licensed GPL-3.0-or-later with the rest of Lince. | DOC (F1, F6) |
| TimeZoneBinary (…080E) | Local time, location name list | Svc `time:*` (the time service reads the rules itself) | **S**: IANA tzdata (public domain), compiled with Lince's own build step to TZif (zic is also public domain), parsed by the HLE time service. Built as an archive only if a title mounts it directly. | DOC (F1), SRC (F7) |
| NgWord / NgWord2 (…0806, …0823) | Profanity filtering of user text | Svc `ngc:u`/`ngct:u`. Whether any title mounts the archive directly (cmd 202) is UNKNOWN. | **O**: no matches, input returned unchanged. A minimal, well-formed archive with no entries is written by Lince only if a title mounts it. | SRC (F8), DOC (F1) |
| SystemVersion (…0809) | Version checks | Svc `set:sys` firmware-version queries; archive mount | **O**: a fixed, documented Lince version profile (0x100-byte layout, no digest on Switch 2) | DOC (F1), SRC (F3) |
| MiiModel (…0802), MiiModel for Switch 2 (…0849), MiiBody (…0847) | Rendering Mii characters | cmd 202 mount, then the game's own renderer | **U** for content. …0849 is served as an **empty archive** so that the title does not abort (F3). The formats are undocumented. An original model set would come much later (M13). | DOC (F2), SRC (F3, F9) |
| Mii database (service-side) | Choosing or showing a user's Mii | Svc `mii:u` | **O**: original default characters generated by Lince | RECALL |
| AvatarImage (…080A) | Profile icons | Svc `acc` GetImage | **O**: avatar images generated by Lince (JPEG) | DOC (F2), RECALL |
| ErrorMessage (Switch 1 only, 0100…0801) / ErrorMessageUtf8 (…0835) | Error text shown by the error applet | Used inside the applet only | **U**: the HLE error dialog shows the error code with Lince's own wording | DOC (F16) |
| CertStore (…0800), SsidList, UrlBlackList, BrowserDll / BrowserCoreDll | TLS, Wi-Fi, browser | `ssl`, `nifm`, web applet | **U**: Lince is offline. `nifm` reports no connection, so no certificate bundle is needed. | Lince design (no evidence claim) |
| Face-detection data (…0844), CameraFirmware (…0860) | GameChat camera features | Camera and GameChat stack | **U**: report no camera. The exact "unavailable" result is UNKNOWN (F10). | DOC (F2, F10) |
| JetShared (…081E), FunctionBlackList, ApplicationCompatibilityInfo, Eula, Help, boot or firmware packages | System UI and system policy | System modules only | **U**: not reachable from games, so not provided | DOC (F2) |
| Other `04…` data a title mounts | UNKNOWN | cmd 202 | An ID is served as an empty archive (with a ledger event) only after it is added to an explicit allowlist with a cited observation, as …0849 is (F3). Every other ID gets the distinct "not found" result, never an empty success (ADR-0024; open question 2). | SRC (F3, F4) |

### 4.2 Library applets

All of these are started with `ILibraryAppletCreator::CreateLibraryApplet(AppletId, mode)`,
fed with `PushInData`, run with `Start`, and finished with `PopOutData` and `GetResult` (SB-AM).
Lince decodes both the Switch 1 AppletIds and the Switch 2 variant AppletIds (F11).

| Applet (AppletId) | Game use | Lince approach | Return when cancelled or unavailable | Evidence |
|---|---|---|---|---|
| Software keyboard (0x11; Switch 2 0x01020011, inline 0x01010011) | Names, search, text entry | **O**: HLE UI (host text input dialog), plus the inline keyboard for the modeless variant | Output u32 = 1 (Cancel); inline mode replies DecidedCancel (6) | DOC (F14) |
| Error (0x0E; Switch 2 0x0100000E) | Show a fatal or recoverable error | **O**: HLE dialog with the code; a log entry when running headless | Normal exit, no output | DOC (F16) |
| Profile selector / player select (0x10) | Choose a user account | **O**: with one user, select it at once; otherwise a chooser | The output result field is non-success on cancel. *The exact value comes from memory (RECALL); switchbrew has no page for it.* | SB-AM, RECALL |
| Controller (0x0C) | Controller setup or pairing | **O**: HLE UI. In a non-interactive run, accept the current configuration. | result 0 with playerCount and selectedId. A refusal uses a value other than 0 or 2 (SB-CTRL). | DOC (F15) |
| Auth / parental PIN (0x0A) | Parental PIN check | **O**: parental controls are always disabled, so this returns success. The `pctl` service must report restrictions as disabled in the same way, so the success is consistent and not silent. | Normal | Lince design (no evidence claim); F13 shows only that Lichii has an Auth frontend |
| Cabinet / amiibo (0x0B) | amiibo setup | **U** now, **O** possibly at M13 | ExitReason Canceled; `GetResult` = module 144 desc 3 (0x690) | DOC (F11, F17) |
| Mii edit (0x12; Switch 2 variants) | Create or choose a Mii | **U** (no Mii models) | Canceled (0x690) | DOC (F11, F17) |
| Web (0x13; Switch 2 OpenWeb 0x01000013), Shop (0x14), LoginShare (0x18; Switch 2 0x01000018), WifiWebAuth (0x19), MyPage (0x1A), Lhub (0x35) | Online pages, eShop, account | **U**: offline | Canceled (0x690) | DOC (F11, F17) |
| Offline web (0x17; Switch 2 0x01000017) | HTML manuals stored in the game's own data | **U** now. Future option (not a finding): an HLE viewer for the title's own HTML at M13. | Canceled (0x690) | DOC (F11) |
| Photo viewer / Album (0x15; Switch 2 0x01000015) | Show screenshots | **U** | Canceled (0x690) | DOC (F11) |
| Net connect (0x0F) | Ask the user to connect to a network | **U**: stays offline | Canceled (0x690). The game then sees no connection through `nifm`. | DOC (F11), SRC (F13) |
| GameChat applets (Switch 2 0x01000065–0x01030065) and `chat:u` | Voice and video chat | **U** | UNKNOWN. The service must not hang (F10). | DOC (F10, F11) |
| Splay (0x64; Switch 2 0x01000024/0x01030024), Eula (0x01000028), PromoteQualification (0x22) | UNKNOWN | **U** | Canceled (0x690), not yet checked against any title | DOC (F11) |
| Any other AppletId | — | **U**, plus a ledger event (ADR-0024: no silent success) | Canceled (0x690), never a zeroed "OK" output | Lince design (ADR-0024), motivated by F13 |

## 5. Open questions

1. **Font licensing and metrics.** See D-009 point 1 (OFL-1.1, subsetting, Reserved Font
   Names). The metric differences from the console fonts are not measured, and fitting the
   subsets into 0x1100000 bytes needs a small PoC.
2. **Correct "not found" result for `OpenDataStorageByDataId`.** Lichii admits it guesses (F4).
   This is UNKNOWN until it is observed by Lince's own testing on legally owned hardware or
   found in public documentation.
3. **Return value when GameChat or the camera is unavailable.** Is there a result that
   initialization accepts as "feature off", instead of a stub session (F10)? UNKNOWN.
4. **Do Switch 2 titles really pass the variant AppletIds** (0x01020011 and similar) to
   `CreateLibraryApplet`, or only the Switch 1 values? SB-AM lists the variants, but no source
   shows which one a game sends.
5. **Player select output layout** (result value on cancel) is RECALL only. It needs a public
   source.
6. **Nintendo Extended codepoint list.** Lince needs a public, non-firmware source listing the
   private-use codepoints games use for button glyphs. If none exists, the glyphs stay
   **UNKNOWN** and render as empty boxes. A list must not be taken from firmware fonts
   (LEGAL-52).
7. **Shared-font obfuscation header.** Switch 1 BFTTF wraps each font in an 8-byte XOR header
   (SRC, Lichii@`0da698bb98c6`:`src/core/hle/service/ns/platform_service_manager.cpp`, which
   calls the transform a key and encryption). It is a fixed constant in a public open-source
   project, not a console key, and Lince would apply it only to fonts it ships itself, but the
   maintainer must decide whether it falls outside LEGAL-02 and LEGAL-05 before anything is
   implemented (decision D-009).
8. Nothing in this task required protected material. Mii and Mii-body model content for Switch
   2 and the camera or face data are left **unavailable** rather than recreated, because their
   formats are known only from firmware.

## 6. Affected ADRs, PoCs and backlog

- **ADR-0029 / LEGAL-06:** every row in §4 is O, S or U. No row reads firmware.
- **ADR-0024:** the applet dispatcher and the fsp-srv data mount report unknown IDs explicitly
  (ledger event plus Canceled or not-found result) instead of the fork's zeroed or empty
  success (F3, F13).
- **doc 10 §3:** replace the rows on fonts, time zone, profanity, avatar and applets with §4.
  Add the Switch 2 rows (Mii model for Switch 2, GameChat, camera). Fix the keyboard evidence
  (F18).
- **doc 07 M13:** Cabinet, Mii edit, offline-web viewer and original Mii models are candidates
  for M13.
- **New backlog candidates:** a font substitution PoC (fit and metrics, open question 1); an
  `assets/system/` layout with license files (LEGAL-34); a tzdata-to-TZif build step.
