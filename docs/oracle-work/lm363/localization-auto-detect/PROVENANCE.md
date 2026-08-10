# Lunar Magic 3.63 localization auto-detection oracle

This fixture records direct decompilation evidence from the labeled `Lunar Magic.exe` program on
the local Ghidra MCP server at port 8089. It contains no Lunar Magic binary or language-resource
payload.

`EnumerateAvailableLanguageModules` at `004D7940` adds built-in English as `(Default)`, scans only
`sysLMLanguage\\*.dll`, requires resource type `$01F4` IDs `$0DB7` and `$0DB6`, requires the first
resource to begin with little-endian `$C001BABE`, accepts at most `$410` bytes of metadata, removes
an optional UTF-8 BOM, and splits four newline-delimited metadata fields after the module filename.

`AutoDetectAndLoadLanguageModule` at `004D7360` treats persisted `(Default)` and `(AutoDetect)` as
distinct modes. Auto detection compares up to 64 preferred UI-language tags case-insensitively,
first in complete form and then after truncating each installed tag at its first hyphen. A match
persists the selected module filename; no match retains built-in English. `GetPreferredUiLanguagesAsUtf8`
at `004DB810` bounds the Windows preferred-language multi-string to `$600` UTF-16 units and falls
back through `BuildFallbackPreferredUiLanguageList` at `004DB640`.

`ValidateLanguageModuleChecksum` at `004D7010` decodes every byte before the final 64-byte trailer,
accumulates the recovered position-dependent transform, reads the stored 32-bit checksum at
`file_size - $38`, and unloads the module on mismatch. These details retain the original DLL ABI
for later compatible resource loading; the Rust catalog remains a bounded clean-room format.

The complete transform was recovered headlessly from the labeled project on 2026-08-10. For byte
offsets whose bit 1 is clear, even offsets contribute `rol8(byte, 2) ^ $46`, while odd offsets
contribute the wrapping negation of `rol8(byte, 4) ^ $77`. Offsets whose bit 1 is set contribute
`u8(byte * -$80 + (byte >> 1) - $17) ^ $71`. Contributions accumulate with wrapping 32-bit
addition. The final 64 bytes never participate; the stored little-endian dword is at
`file_size - $38`.

`EnumerateAvailableLanguageModules` was also re-decompiled in full. It requires resource type
`$01F4`, ID `$0DB7` to contain at least the little-endian `$C001BABE` marker, then reads bounded
UTF-8 metadata from ID `$0DB6`, removes an optional BOM, normalizes CRLF delimiters, and retains
four fields after the module filename: display name, version, locale tag, and code page.

`LoadLanguageStringResources` at `$004D6D40` requires three more type-`$01F4` resources. `$0DAC`
is copied, then bytes 1 through end are decoded in place as
`((encoded ^ $92) - previous_decoded) + $34` before raw-DEFLATE expansion. `$0DAD` begins with a
little-endian declared count followed by 32-bit string offsets; `$0DAE` contains parallel 32-bit
lengths. The effective count is the minimum of the declaration, both complete table extents, and
`$16EE` (5,869). Each entry is retained only when offset plus length is in the inflated pool and
the following byte is NUL. `LoadSelectedLanguageModule` at `$004D7110` requires this string load
and the whole-file checksum before publishing the module.

The loader then applies two fixed-buffer safety tables before publication. `$005E6420..$005E685F`
contains 272 `(string index, exclusive byte-length ceiling)` pairs (raw-table SHA-256
`7c32c38900036af820ddda9311a66617c00c1882475182fd80a28358d7097903`).
`$005E6398..$005E641B` contains 22 `(inclusive start, exclusive end, exclusive byte-length
ceiling)` triples (raw-table SHA-256
`cabeed1bfddffaeca5550aababd4c59da60a901cae92b39f5a9f2950e279978e`). The loader clears both
offset and length when `length >= ceiling`; indices beyond the effective string count are ignored.

`CreateMainApplicationMenu` at `$00447540` pairs each nonzero offset-table slot with an immediate
built-in English fallback before calling `AppendUtf8MenuItem`. The retained headless extractor
`tools/DumpLocalizedStringFallbacks.java` walks that exact pairing. The typed Rust catalog currently
uses the following semantically equivalent slots; all other Rust-only keys deliberately retain
built-in English:

| Original index | Typed key(s) | Built-in fallback evidence |
|---:|---|---|
| `$000A..$000D` | `MenuFile`, `MenuEdit`, `MenuView`, `MenuEditors` | File, Edit, View, Editors |
| `$0010` | `MenuHelp` | Help |
| `$0011` | `FileOpen` | Open ROM |
| `$0014..$0015` | `FileSave`, `FileSaveAs` | Save Level to ROM / as |
| `$001E` | `ToolsTestRomInEmulator` | Emulator submenu |
| `$001F` | `FileExpandRom` | Expand ROM submenu |
| `$0023..$0024` | `FileOpenRecent`, `FileQuit` | Recent Files, Exit |
| `$0032` | `FileAnalyzeLevelUsage` | Analyze Resources in Levels |
| `$0036..$0037` | emulator action / chooser | Run ROM in Emulator, Setup Emulator |
| `$004E..$0051` | full restore, restore, create/apply IPS | matching restore/IPS actions |
| `$0055..$0059` | Undo, Redo, Cut, Copy, Paste | matching Edit actions |
| `$006C..$006F` | Layer 1, Layer 2, Layer 3, Sprites | matching View layers |
| `$0081` | `ViewSpecialWorldPassed` | Special World Passed |
| `$0118..$0119` | `HelpTopics`, `HelpAbout` | Contents, About `%s` |

Conversion removes Windows mnemonic ampersands and tab-delimited accelerator text, collapses `&&`
to a literal ampersand, normalizes a trailing `...` to `…`, and substitutes `Lunar Magic Rust` for
the About `%s`. A missing, cleared, or empty original slot falls back independently to the typed
English value.

`FindLocalizedDialogResourceId` at `$004D76E0` binary-searches 107 built-in dialog IDs in
`$005E61B8..$005E628D` and returns the corresponding language-DLL ID from
`$005E62A8..$005E637D` only when that module actually contains a type-5 resource with the mapped
ID. The original-ID table's raw SHA-256 is
`24cb467274b98621cbc92985af83fe0e2e5b918f6d95038f17117e58be3cbdfa`; the localized-ID
table's is `c45a14ded0e8e4c062f828a93784ef1a85a4181eb6a4cddb3d55bf6d73b462da`. Re-encoding the
107 Rust pairs as consecutive little-endian words produces those same two hashes.

`ShowLocalizedModalDialog` at `$004D7FE0` and `CreateLocalizedModelessDialog` at `$004D80C0`
fall back independently to the executable's original dialog ID when the map lookup or DLL resource
probe fails. Otherwise they use the mapped type-5 template, either by locking it and calling the
indirect Win32 API or by passing the module plus mapped ID according to the original compatibility
flag. Rust now validates the complete module marker/metadata contract and exposes borrowed bytes
for every present mapped type-5 resource while omitting missing mappings, preserving that exact
per-dialog fallback boundary without loading or executing the DLL. Parsing and applying those
Win32 templates to native Rust dialog controls remains separate unfinished work.

The portable parser now implements both Microsoft-documented binary layouts rather than treating
those payloads as strings: standard `DLGTEMPLATE`/`DLGITEMTEMPLATE` and extended
[`DLGTEMPLATEEX`](https://learn.microsoft.com/en-us/windows/win32/dlgbox/dlgtemplateex)/
[`DLGITEMTEMPLATEEX`](https://learn.microsoft.com/en-us/windows/win32/dlgbox/dlgitemtemplateex).
It consumes style-dependent font fields, `sz_Or_Ord` menu/class/title values, DWORD-aligned control
records, 16-bit standard or 32-bit extended control IDs, UTF-16 captions, and bounded creation data.
Only literal dialog/control text is published; resource ordinals are never misrepresented as text.
Every truncation, invalid UTF-16, invalid extended version, and non-padding trailer rejects.

The ignored local-executable gate parses every one of the 107 mapped type-5 resources from both the
32-bit and 64-bit Lunar Magic 3.63 executables without redistributing their contents. Two direct
procedure/resource bindings are currently converted into typed Rust catalog actions: language
dialog `$042B` IDs 1/2 provide the common OK/Cancel labels, and About dialog `$03F8` IDs 1/`$66`/
`$67` provide its OK, Third Party Enhancements, and Legal Notice labels. Missing or malformed
individual templates retain the typed English fallback. The rest of the decoded control inventory
still needs semantic binding to the corresponding native editor forms.

Converted catalogs now also retain the complete literal dialog inventory instead of discarding
unbound controls. The optional append-only `LMDLG001` section stores at most 4,096 bounded records
under `(original dialog ID, template item index, control ID)`, so repeated Win32 control IDs remain
distinct and old `LMLOC001` files without the section retain their exact byte encoding. Dialog
titles use a canonical sentinel key; malformed keys, duplicate records, invalid UTF-8, NULs,
oversized strings, truncation, foreign extension magic, and trailing bytes reject atomically.

The first complete native consumer is Modify Secondary Entrances dialog `$03F1`. Its native title,
Clear Slot (`$66`), Clear All Slots (`$65`), Destination Level Number (`$6C`), Screen Number of
Entrance (`$DB`), X (`$67`), and Y (`$69`) captions resolve from the converted original template
and fall back independently to the built-in English labels. The remaining native forms and a
retained live language-DLL Wine gesture are still required before Localization can pass.

Two more procedure-bound forms consume the same inventory. The native undo-history preference is
the extracted equivalent of General Options `$041F`: its title, maximum-undo label `$66`, OK ID 1,
and Cancel ID 2 resolve independently. Graphics compression migration consumes Change Compression
Options `$0416`: title, LZ Compression Type `$65`, original LZ2 `$294`, LZ3 `$296`, warning `$69`,
OK, and Cancel are bound. The same selector now exposes optimized LZ2 `$295` as a distinct target:
authenticated LZ2 Orig defaults to Speed, and selection routes to the existing revision-bound
`InstallLz2SpeedRuntime` transaction. That transaction installs only the decoder for payload-
compatible LZ2 Orig or performs the complete LZ3-to-Speed migration.

Setup SNES Emulator `$0407` now has a native in-app configuration consumer. Its title, emulator
path `$66`, command-line arguments `$68`, OK, and Cancel resolve from the original dialog inventory
with independent fallback. The native editor deliberately represents arguments one per line so
each becomes one direct process argument without shell parsing; it additionally exposes stable ID,
display name, optional working-directory template, and portable opened/saved/level subscriptions.
