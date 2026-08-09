# Lunar Magic reverse-engineering ledger

This ledger accompanies the live Ghidra database. Names are intentionally conservative.

Confidence levels:

- **High**: behavior is directly supported by decompilation, constants, strings, and/or xrefs.
- **Medium**: subsystem is clear but some parameters, flags, or edge cases remain unresolved.
- **Tentative**: useful working hypothesis; do not copy into a reimplementation without more evidence.

## Coverage

Latest measured internal Functions-table coverage: **4,027 named / 4,027 listed**, with **zero `FUN_...` placeholders remaining**. The earlier 3,912-function audit omitted an address-taken standard-object renderer cluster and 48 MSVC startup initializers that Ghidra had decoded as instructions without creating all required function bodies. A later bitmap audit found the same condition for the address-taken Other Options dialog callback at `$004F1FA0`; all three groups are now promoted, named, documented, and verified in the live Ghidra Functions table on port 8089. Ghidra's separate total-function count is **4,413**, including **386 imported/external symbols**.

Dynamic validation now includes Lunar Magic 3.63's documented `-TransferCredits` operation under
Wine. A pristine-to-pristine transfer expands the headered destination from `$80200` to `$100200`,
installs one RATS block at logical `$80000` with payload `$80008..$80759`, replaces the fixed
credits runtime and 256-entry offset table, and preserves the exact 8,192-word editor model. The
Rust pristine and installed decoders produce identical `LMCREDT1` and row-addressable `LMOBS1`
artifacts; the captured oracle binds the exact input/output hashes, 144 physical changed ranges,
new ownership, and zero semantic differences.

### Wine differential evidence

Lunar Magic 3.63's recovered command-line dispatcher was exercised under Wine against the pristine
US SMW ROM whose unheadered SHA-256 is
`0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b`.
`-ExportMultLevels ROM OUTPUT_PATTERN 0` exported all 512 slots as version `0x0363` MWL files.
Every file decoded and re-encoded byte-for-byte through `lm-level`. Passing mode `1` selected the
modified/eligible-only path and exported zero levels from the pristine image.

Command-line mode automatically accepts Lunar Magic's prompt to add a required 512-byte copier
header. Differential workflows must therefore operate on a disposable headered copy, even for
apparently read-only exports. A pre-headered input remained byte-identical during the 512-level
export. This behavior and the observed mode meanings are also recorded on
`ExportAllLevelsToDirectory` (`00485720`) in Ghidra.

Live decompilation of `ExportAllLevelsToDirectory` confirms that ordinary SMW modified-only
selection first loads the 512-entry Layer 1 PC-offset table and exports a slot when its payload is
at or beyond active descriptor entry `0x31`. `CheckRomOffsetCanBeModified` (`004425B0`) proves the
same lower-bound comparison; the SMW-US revision-0 descriptor boundary is the original headerless
ROM length, `0x80000`. Against the retained installed Layer 3 fixture, Wine mode `1` exported only
`Modified 000.mwl`, while the clean-room predicate selects exactly slot `000`; the corresponding
pristine before-ROM selects no slots.

`InsertMultipleLevelsFromDirectory` (`00485180`) enumerates `*.mwl`, ignores directory/system
entries, counts hidden files as skipped, calls `ImportLevelFileAutoDetect`, and attempts
`SaveLevelToRom(1)` for every successfully decoded file. Import or save failure increments a
failure count and does not abort later files. The clean-room shell follows this per-file commit
boundary and reads each target level from the MWL header rather than retargeting it to the current
editor selection.

The same pristine headered copy produced three additional exact compatibility corpora:

- `-ExportAllMap16` emitted a 651,760-byte `LM16` container. Its eight directory entries describe
  a `0x80000` combined tile bank, `0x10000` Acts Like bank, aliased `0x40000` foreground and
  background halves, an absent optional extended bank, and `0xF000`, `0x100`, and `0x40`
  auxiliary sections. The lossless `Lm16Map16File` Rust decoder preserves those intentional
  aliases and re-encodes the real file byte-for-byte. Complete exports deliberately zero the
  first `0x1000` foreground-definition bytes (tiles `$0000-$01ff`), while imports leave the
  corresponding built-in ROM definitions unchanged.
- `-ExportSharedPalette` emitted the recovered 2,018-byte legacy palette backend containing 1,009
  colors. `SmwPaletteFile` re-encoded it byte-for-byte.
- `-ExportGFX` emitted 52 separate 4-bpp planar files. Every file survived planar decode and encode
  byte-for-byte.

### First real save/install cases

Command-line imports of Lunar Magic's own unchanged level `000` and `105` MWL exports were run
against separate disposable pristine-US copies. Both saves expanded the logical ROM from `0x80000`
to `0x100000`, installed 13 validated RATS blocks, retained the original additive checksum
`0xA0DA`, and produced a matching SNES checksum. The two before/after pairs and their semantic
observations replay successfully through `oracle-verify-suite`.

Re-exporting level `105` proved that its Layer 1, Layer 2, and sprite payload bytes were unchanged.
Only the second u32 of each MWL common-prefix section moved, establishing it as a 24-bit source SNES
address:

- Layer 1: `$0688DD` to `$108008`
- Layer 2: `$FFD900` to `$0CD900`
- sprites: `$07C4CA` to `$108640`

This dynamic evidence is recorded on `SaveLevelToRom` (`00483240`) and
`ExportBinaryMwlLevelFile` (`004797D0`) in Ghidra. The Rust MWL observation now records a separate
payload SHA-256 so address relocation cannot be mistaken for content mutation. A content-addressed
`rats-observe` workflow likewise records every installed tag's logical range, length, and payload
SHA-256.

The extracted Lunar Magic 3.63 help confirms the supported level-oracle syntax as
`-ExportLevel ROM MWL LevelNumber` and `-ImportLevel ROM MWL [LevelNumber]`. This is now exercised
by an opt-in Wine integration test over the Rust complete Layer 3 installation: the built
application installs all five runtime payloads plus the expanded-settings allocation, repairs the
checksum, and saves a 1 MiB ROM; Lunar Magic reopens that ROM and exports level `$105`; and the Rust
MWL decoder then round-trips the resulting version `$0363` file byte-for-byte. The ordinary
cross-platform test suite leaves this case ignored because it requires the locally supplied
proprietary executable and Wine, while the explicit `--ignored` run preserves a reproducible
dynamic-oracle gate.

### Function-boundary completion audit

The executable-gap audit found 143 regions containing decoded instructions outside recognized function bodies. Direct-control-flow analysis found no direct `CALL` target into those regions; 454 direct `JMP` targets are switch arms or disconnected basic blocks owned by surrounding functions. An aligned initialized-data pointer scan found two genuine function-entry clusters:

- `005E87F8` contains the standard-object renderer dispatch table. Its 78 initialized entries now all resolve to recognized functions; 65 function bodies had to be promoted during this audit (including the separately probed slot-zero entry). All recovered entries use the common `void(startColumn, packedDimensions, levelMap16Base)` ABI and now record their table slot and Map16 side effects.
- `00597610` contains 48 MSVC C/C++ startup initializer pointers. These short zero-argument routines initialize Lunar Magic editor globals before the main UI starts. They are now represented as `InitializeStaticEditorState00` through `InitializeStaticEditorState47`, with `void(void)` prototypes and startup-table evidence.

Coincidental integer values inside strings/constants and targets belonging to CRT internal jump tables were deliberately not promoted. This prevents false function boundaries while ensuring every confirmed direct or address-taken entry point is visible in Ghidra's Functions window.

The clean-room differential-testing plan is maintained in `REIMPLEMENTATION_TEST_MATRIX.md`. It defines fixtures, oracle capture, byte-for-byte ROM comparisons, codec round trips, UI-model state checks, malformed-input cases, and subsystem release gates for feature-parity work.

### Native shared-palette ROM storage

`SaveSharedPalettesToRom` (`0049D940`) writes layout-descriptor entry `$29`.
For SMW US revision 0 the static headered descriptor value is `$32A0`, proving
logical PC `$30A0`. The pristine ROM's `$7E2` bytes at that location are
byte-identical to Lunar Magic's retained `-ExportSharedPalette` output.

The legacy backend writes `$7E2` bytes beginning at the main palette buffer.
The expanded backend is selected when descriptor entry `$4A` contains marker
`$C2`; its SMW US logical offset is `$77570`. It writes `$810` contiguous ROM
bytes from the auxiliary buffer: `$10` auxiliary bytes followed by the `$800`
main palette. Native `.smwpal` files deliberately reverse those regions,
storing the `$800` main palette first and the `$10` auxiliary region last.

Rust now performs that ordering conversion explicitly and validates the
installed backend marker. For an already-installed backend it saves the fixed
table and checksum as one undoable transaction. Importing an expanded
`.smwpal` into pristine SMW US revision 0 now installs all three recovered JSL
sites, the two fixed helper stubs, the exact `$60`-byte runtime, and the
`$600`-byte (512 x 24-bit) per-level custom-palette pointer table as one
identity-checked transaction. The new table is initialized to null pointers;
Lunar Magic's retained oracle has a non-null first entry only because that
capture additionally imported level 000's custom palette. Fixed runtime
regions are tested byte-for-byte against the Wine-produced ROM, while semantic
reopen, checksum repair, late-failure rollback, and exact undo are covered
independently. `smw-shared-palette-export/import` expose the revision-bound
operation, and the application shell uses the same installation, reopen, and
revision guards.

## C++ object, RTTI, and vtable audit

The executable does **not** contain an identifiable set of Lunar Magic-owned MSVC polymorphic classes. A raw PE audit found no MSVC `TypeDescriptor` names (`.?AV...@@` / `.?AU...@@`), no credible Complete Object Locator records, and no application vftable run in `.rdata`. The few contiguous code-pointer tables are CRT initialization tables, delay-load thunks, object-render dispatch tables, or repeated error stubs. This is consistent with RTTI being disabled or with most application code being procedural; it is not evidence that the program was written entirely in C.

The confirmed virtual dispatch is Direct3D 9 COM. Those vtables reside in objects returned by `d3d9.dll`, not as static vftables in the Lunar Magic image. Ghidra now contains partial, offset-accurate `LM_IDirect3D9Vtbl`, `LM_IDirect3DDevice9Vtbl`, `LM_IDirect3DTexture9Vtbl`, and `LM_IDirect3DVertexBuffer9Vtbl` structures. Named slots include `GetAdapterCount`, `GetDeviceCaps`, `GetAdapterMonitor`, `CreateDevice`, `Reset`, `Present`, `CreateTexture`, `CreateVertexBuffer`, `BeginScene`, `EndScene`, `Clear`, `SetTransform`, `SetRenderState`, `SetTexture`, `SetTextureStageState`, `SetSamplerState`, `DrawPrimitive`, `SetFVF`, `SetVertexShader`, `LockRect`, `UnlockRect`, and vertex-buffer `Lock`/`Unlock`.

The corresponding `LM_IDirect3D9`, `LM_IDirect3DDevice9`, `LM_IDirect3DTexture9`, and `LM_IDirect3DVertexBuffer9` interface shells have valid typed `lpVtbl` links. These links were explicitly revalidated after vtable reconstruction; none remain as Ghidra `-BAD-` components. A label at `00597000`, `g_RttiAudit_NoApplicationTypeDescriptors`, anchors the negative RTTI result in the listing. It is an audit marker rather than an RTTI object—the address does not contain a fabricated descriptor.

The renderer allocation itself is a plain 0x1D8-byte `LM_Direct3DRendererContext`, not a polymorphic C++ object: offset zero contains `IDirect3D9 *`, not a Lunar Magic vptr. The recovered structure contains an exact 0x130-byte `LM_D3DCAPS9`, a 0x38-byte `LM_D3DPRESENT_PARAMETERS`, window handles, client dimensions, adapter state, the device pointer, a tiled `IDirect3DTexture9 **` array, an `IDirect3DVertexBuffer9 *`, tile dimensions/counts, cached source dimensions, and render-mode state. Its typed constructor-like allocator prototype is `LM_Direct3DRendererContext * __cdecl CreateDirect3DRendererContext(void *ownerWindow)`; `CreateTiledDirect3DSurfaces` also has a typed four-argument prototype.

Several renderer helpers use register-resident context parameters, but they do not follow the MSVC `__thiscall` contract. Depending on the routine, the context arrives in EAX, ESI, EDI, or EDX; `RenderTiledDirect3DSurfacesToWindow` additionally carries source height in ECX and packed flags in AL. These locations are documented at each function rather than incorrectly forcing an implicit `this`. Its recovered fastcall prototype now names `sourceHeight`, `context`, `sourcePixels`, `sourceWidth`, `zoomXPercent`, and `zoomYPercent`, while retaining the separate undocumented AL flag input. The three owned context globals are typed and named as `g_pLevelEditorD3DRenderer` (`00e278e8`), `g_pOverworldD3DRenderer` (`00e27eb8`), and `g_pBackgroundD3DRenderer` (`00e27cac`). The dynamically resolved factory pointer at `0060b6b0` is named `g_pfnDirect3DCreate9`.

Most recent address-ordered batch (`004026f0`-`004039b0`) recovered dialog numeric/hex formatting helpers, shared tooltip management, edit-control subclasses, and two randomized embedded-resource integrity checks.

The `00403a50`-`00407a40` batches recovered the level-mode/property dialog: entrance and completion actions, FG/BG indices and offsets, horizontal/vertical scroll modes, Layer 3 choices, music and tileset names, manual object/sprite command parsing, and level-mode table editing. A retained Lunar Magic 3.63 command-line export additionally proves that its first open of a headerless pristine image adds the exact canonical 512-byte copier header beginning `40 00 00 00 00 00 00 00 AA BB 04 00`, while an image already carrying that canonical header retains it byte-for-byte. `AddCopierHeaderToRomFile` at `$0044E300` and the IPS-normalization helper `ToggleSnesCopierHeader` at `$0041E450` recover the complete variant rule: write little-endian `(logical_len >> 17) << 4` at bytes 0-1; when internal map-mode bit zero is set, write `$30,$80` at bytes 2-3; write `$AA,$BB,$04` at bytes 8-10; zero everything else. There is no independent original conversion dialog. The Rust profile now synthesizes this structure from the authenticated open image, including expanded LoROM and SA-1 sizes; the application/native transaction adds or replaces it without changing logical bytes and is the actual producer used by the reciprocal Wine gate. The four logical entrance planes export identically from both physical forms.

The Layer 1/2 settings dialog at `00413310` was subsequently recovered byte-for-byte. Layer 1's
four-way vertical-scroll choice is legacy level-header byte 4 bits 4-5. Layer 2 normally uses the
main-entrance position byte's high nibble as one of sixteen paired presets; SMW's effective
horizontal selectors are `[2,2,1,0,1,2,1,0,0…]` and vertical selectors are
`[3,1,1,0,0,2,2,1,0…]`. Lunar Magic's “separate settings” flag is MWL level-header byte `$11` bit
7. In that mode, its low five bits hold the horizontal selector, the position high nibble holds
vertical bits 0-3, and byte `$11` bit 6 holds vertical bit 4; bit 5 is unrelated and preserved.
Dialog save code at `004136d6`-`004137b9` and a Rust MWL import/re-export through Lunar Magic 3.63
both verify this packing. The nearby `$00600B36/$0060AC37` fields instead control extended BG
initial position and must not be mistaken for camera scroll rates.

The screen-exit object boundary is now independently recovered across
`DeduplicateScreenExitObjectsByScreen` (`00437190`),
`BuildPackedScreenExitArrayFromObjects` (`0043acd0`), and
`SetScreenExitObjectForScreen` (`0043ad90`). Layer 1 command-zero records use parameter `0` for a
four-byte compact exit and parameter `2` for a five-byte extended exit. Byte 0's low five bits are
the source screen. The compact form stores the destination/flag high nibble in byte 1 and its low
byte in the first extension; the extended form stores the complete high byte in a second extension.
Lunar Magic keeps at most one exit per screen, unconditionally sets packed destination flag
`$0400`, selects the compact form when the resulting destination's top nibble is clear, upgrades
to the extended form otherwise, and preserves byte 0's unrelated new-screen bit. A four-cycle
Wine import/re-export over a real pristine-ROM record confirms screens `$00/$1F`, requested values
`$0000/$0FFF/$1000/$FFFF`, both representation shapes, and the `$0400` canonicalization.

## Confirmed subsystem map

| Address | Ghidra name | Role | Confidence |
|---|---|---|---|
| `00401bb0` | `DecodeLz2CommandStream` | Core LZ2 command decoder | High |
| `00401dd0` | `ExpandPackedGraphicsRows` | 24-byte to 32-byte graphics-row expansion | High |
| `00401e50` | `DecodeTerminatedByteRunRle` | `FF FF`-terminated byte RLE decoder | High |
| `00401f40` | `DecodeSizedByteRunRle` | Output-length-bounded byte RLE decoder | High |
| `00402040` | `DecompressLunarMagicDataStream` | General decompression front end | High |
| `00402640` | `DecompressCodec2DataStream` | Third-codec decompression front end | Medium-high |
| `00401000` | `InitializeLegacyNonClientRenderingWorkaround` | Detects old-Windows caption rendering issue | High |
| `004010e0` | `CalculateNonClientCaptionBounds` | Computes custom caption rectangle from window styles | High |
| `00401140` | `DrawNonClientCaptionButtons` | Draws close/maximize/minimize caption controls | High |
| `00401250` | `DrawWindowCaptionIcon` | Draws the small caption icon | High |
| `004012b0` | `CreateSystemCaptionFont` | Creates the current system caption font | High |
| `00401300` | `RenderCustomWindowCaption` | Renders caption gradient, icon, buttons, and title | High |
| `00401600` | `DeleteCachedCaptionBitmaps` | Releases two cached non-client bitmaps | High |
| `00401630` | `PaintCustomNonClientCaption` | Double-buffered custom caption painter | High |
| `00401770` | `HandleNonClientPaintWorkaround` | WM_NCPAINT compatibility path | High |
| `004018c0` | `HandleNonClientActivationWorkaround` | WM_NCACTIVATE compatibility path | High |
| `00401960` | `HandleFrameStyleChangeWorkaround` | Rebuilds cached frame after style changes | High |
| `00401a00` | `HandleLegacyCaptionSystemCommand` | Toggles compatibility caption via system menu | High |
| `00401ab0` | `DecodeLz2AlternatingWordFill` | Alternating two-byte LZ2 fill primitive | High |
| `00401b10` | `DecodeLz2IncrementingByteFill` | Incrementing-byte LZ2 fill primitive | High |
| `00401b50` | `DecodeLz2BackReference` | LZ2 dictionary-copy primitive | High |
| `00402950` | `ShowRomOffsetRangeError` | Formats and displays ROM offset bounds failure | High |
| `0043e920` | `ShowLunarMagicErrorByCode` | Central signed error-code to localized-message dispatcher | High |
| `00435650` | `SerializeLevelObjectList` | Internal object nodes to SMW variable-length object stream | High |
| `00441160` | `DecompressGraphicsByConfiguredFormat` | Compression-mode dispatch for graphics payloads | Medium-high |
| `0047da40` | `ExtractAllGFXFiles` | Standard GFX extraction to `Graphics/` or `AllGFX.bin` | High |
| `0047e270` | `CompressAndAllocateGraphicsStructure` | Compress, allocate, and insert GFX/ExGFX structures | High |
| `0047e720` | `InsertAllGFXFiles` | Standard GFX validation and insertion | High |
| `0047eff0` | `ExportExtendedGraphicsFromRom` | ExGFX enumeration and extraction | High |
| `0047f470` | `ImportExtendedGraphicsIntoRom` | ExGFX enumeration, validation, compression, and insertion | High |
| `0047fce0` | `ConvertRomTo64MbitExLoROM` | 8 MiB ExLoROM conversion workflow | High |
| `00480620` | `RunChangeCompressionCommand` | Compression-options command and restore description | High |
| `00483030` | `FindDuplicateRatsPayload` | Exact payload deduplication against known RATS blocks | High |
| `004831e0` | `AllocateRatsPayloadOrReportError` | RATS allocation wrapper with localized failure | High |
| `00483240` | `SaveLevelToRom` | Main level serialization/allocation/table-update transaction | High |
| `00491080` | `PromptToSaveModifiedLevel` | Modified-level save prompt workflow | High |
| `004a6960` | `GetLoadedRomSize` | Size abstraction for file-backed or memory-backed ROM | High |
| `004a6bc0` | `SeekRomStream` | Shared seek abstraction for CRT/Win32/memory ROM sources | High |
| `004a6c10` | `WriteRomStream` | Shared write abstraction with image growth and restore tracking | High |
| `004a6d60` | `ReadRomStream` | Shared read abstraction for CRT/Win32/memory ROM sources | High |
| `004a6e10` | `ZeroRomRange` | Clears a ROM range while coordinating restore tracking | High |
| `004a7a40` | `RepairRatsEraseRange` | Detects nested/partial RATS erasures and repairs the range | High |
| `004a7930` | `AppendRatsLogEntry` | Timestamped `RATS.log` writer | High |
| `004a82e0` | `FindAndWriteRomFreeSpace` | Constrained free-space search and payload write | High |
| `004a8810` | `AllocateRomSpaceWithExpansion` | Mapper-aware free-space search with optional ROM expansion | High |
| `004a8d50` | `EraseRomDataSafely` | RATS-aware erase/rollback wrapper | Medium-high |
| `004a9640` | `EncodeLz2CommandStream` | Core LZ2 command encoder | High |
| `004a9d30` | `EncodeByteRunRle` | `FF FF`-terminated byte RLE encoder | High |
| `004a9f70` | `EncodeLengthDelimitedRle` | Length-delimited byte RLE encoder | High |
| `004aa170` | `CompressLunarMagicDataStream` | General compression front end | High |
| `004ab190` | `CompressCodec2DataStream` | Third-codec compression front end | Medium-high |
| `004d9960` | `ShowLocalizedMessageBox` | Internal/localized string conversion and `MessageBoxW` wrapper | High |

## Added domain types

- `RatsTagHeader` (8 bytes): `STAR` signature, payload-size-minus-one, and one's-complement size.
- `RomCompressionMode` (32-bit enum): `LZ2_ORIGINAL=0`, `LZ2_SPEED=1`, `LZ3=2`. Live
  decompilation of the mode-2 wrapper, encoder, decoder, and copy helpers confirms the third codec
  is Lunar Magic's LZ3 variant: LZ2-style headers, zero-fill command 3, one-byte relative or
  two-byte absolute dictionary operands, bit-reversed forward copy, and reverse commands 6 and 7.
  For SMW US revision 0, Lunar Magic stores this mode in the low nibble at logical `$07FFEB`.
  Modes 1 and 2 replace the fixed routine at `$0038E3` with a JSL to an exactly owned RATS runtime
  followed by RTS. The standard-LoROM mode-1 body is `$1C0` bytes (CRC32 `$5D3CAC46`); mode 2 is
  `$2AB` bytes (CRC32 `$DCB7727E`). Both end in `LM 01 01`. Mode 0 instead requires the original
  five-byte routine. `RemoveGraphicsCompressionRuntime` (`$00480060`),
  `InstallGraphicsCompressionMode1Runtime` (`$00480120`),
  `InstallGraphicsCompressionMode2Runtime` (`$00480220`), and
  `ConvertRomGraphicsCompressionMode` (`$00480320`) establish the removal/install/conversion
  lifecycle. A fresh command-line `LC_LZ2_Speed` oracle proves modes 0 and 1 share payload encoding:
  only the hook, runtime allocation, metadata nibble, and checksum compensation change.
  Mode 2's authenticated standard-LoROM runtime is now retained as an immutable template. A Rust
  staging component converts all 50 split-plane standard pointers plus the independently addressed
  GFX33/GFX32 startup pair to LZ3, keeping the latter in their required shared bank. Lunar Magic
  recognizes the staged ROM as already `LC_LZ3` and re-exports all 52 files byte-identically. The
  completed application transaction now also walks every non-null compressed `$80..$FFF`
  ExAnimation/ExGFX pointer and converts both installed overworld-event streams before publishing
  the runtime/metadata switch. Raw `$60..$63` files remain unchanged because the selected codec does
  not apply to them.
- `LevelObjectNode` (currently 29 recovered bytes): linked-list pointer plus the serialized command bytes and encoding-variant field. Unknown regions remain explicitly named as byte arrays. The type is applied at the Layer 1 list head (`0060b6b8`).
- `ManualEditorCommandBuffer` (16 bytes): shared encoded-command workspace used by the manual object and sprite editors, applied at `008636c4`.
- `LayerScrollMode` (8-bit enum): all 32 scroll-mode values, including automatic directional modes and unused slots.
- `Layer3StartPosition`, `Layer3TilemapSize`, and `Layer3LiquidType` (8-bit enums): recovered directly from the Layer 3 configuration selector tables.
- `ExAnimationRecord` (522 bytes / `0x20A`): fixed-size clipboard and editor record. Its raw extent is established; individual fields will be split as their semantics are confirmed.
- `ExAnimationRecord[64]` is applied at `00907398`, establishing the complete `0x8280`-byte editor/clipboard array rather than leaving overlapping scalar globals.
- `PackedScreenExit[32]` is applied at `008f4658`. Its single 32-bit field is intentionally left encoded until all flag meanings are verified.
- The secondary-exit database is represented truthfully as six named parallel `byte[8192]` globals at `0090f750` through `00919750`; Lunar Magic does not store these entries as contiguous C structs in memory.
- `ExAnimationRecord[448]` is applied at `00b56338`, representing seven overworld submaps with 64 reserved slots per submap (`7 * 0x8280` bytes). Runtime logic exposes either 32 or 64 slots depending on submap capabilities.
- `OverworldLayer3Settings[7]` is applied at `00b45fb0`. The recovered 32-byte record contains feature/configuration flags, packed tilemap file/size/position, eight address-layout words, four preservation-only bytes, and four 12-bit Layer 3 graphics indices.
- `RomLayoutDescriptor` is an offset-accurate 3,088-byte (`0xC10`) indexed descriptor with 772 32-bit entries. The active pointer at `009203c8` is typed as `g_pActiveRomLayoutDescriptor`. Individual indexes are documented at their consumers rather than being assigned speculative field names; `BuildMappedRomLayoutDescriptor` proves that most entries are mapper-dependent addresses while twelve indexes are non-address scalars.

The `00407a80`–`00409ea0` block is now named as the ExAnimation selector, address-conversion, clipboard, and tile-remapping subsystem. Clipboard formats explicitly identify single-record and 64-record variants; the shared temporary global-memory handle is labeled `g_pExAnimationClipboardMemory`.

The subsequent editor block through `0040bd30` is also named. `ExAnimationDialogProc` and `ExAnimationFrameEditSubclassProc` have recovered Win32 callback prototypes; frame shifting/rotation, point-and-click highlighting, tooltips, slot serialization, and editable-combo behavior are annotated at their implementations.

The `0040f250`–`00414950` range now identifies the Layer 3 scroll selectors, screen-exit and secondary-exit editors, Change Level Mode dialog, music selector, and General Options dialog. Win32 callback prototypes are applied to `ChangeLevelModeDialogProc` and `GeneralOptionsDialogProc`; packed-exit decoding/encoding behavior is documented at the relevant functions.

`ChangeLevelModeDialogProc` (`00412250`) compares `ClassifyLevelModeLayer2Storage` for the selected
and stored modes before publishing the header. An object-to-tilemap transition requires approval,
zeroes the complete `$800`-byte tilemap workspace, applies descriptor byte
`(old & $FA) | $1A`, and marks the relevant domains dirty. The reverse transition has its own
approval, retains the dormant object workspace, and applies `old & $E0`; no reset occurs while both
modes remain in the same storage class. Rust mirrors that active/dormant boundary in both native
controllers and refuses an unapproved transition before staged state changes.

The `00414bb0`–`004178b0` range identifies interface/VRAM-patch option tooltips, the About dialog and URL clipboard path, and the beginning of the overworld ExAnimation subsystem. `AboutDialogProc` has a recovered Win32 callback prototype. Overworld animation address/frame conversions, remapping, submap selection, slot display, and duplicate-trigger checks are named separately from their level-editor counterparts.

The 600-byte extended dialog template for About resource `$03F8` is now decoded into the retained
`help-about/about-layout.tsv`: a 248×160-dialog-unit parent with ten exact child IDs, classes,
positions, sizes, and roles. `AboutDialogProc` at `00415970` fills the identity, version, build,
programmer, and website controls; website hover changes the link color/cursor, left-click launches
the URL, and right-button release opens its Copy command. Command `$66` opens modal resource `$429`
through procedure `004155C0`, `$67` opens Legal Notice resource `$42A` through `004156C0`, and ID 1
ends only the About dialog. Rust now retains the recovered window proportions, version/build/source
identity, direct URL and copy actions, both auxiliary clean-room notices, and explicit OK dismissal.

The Help-file dispatch is now recovered separately. `OpenLunarMagicHelpFile` at `00440F90` first
asks `BuildLocalizedHelpFilePath` (`004D6C10`) to replace the active language-module extension with
`.chm`; if that file is absent it selects `Lunar Magic.chm` beside the executable. It attempts to
delete the `:Zone.Identifier` alternate stream, then routes the UTF-8 path through
`ShowHtmlHelpFromUtf8Path` at `004E4870`. That wrapper selects `HtmlHelpW` or `HtmlHelpA` and passes
command zero plus data zero. The fastcall argument supplied by both the level (`00497E36`) and
overworld (`00564DB9`) command dispatchers is therefore the owner window, not a topic route. Failed
launch retries through an 8.3 short path at `004E4B30`; missing files and a failed retry use the
original error-code presenter. The retained `help-chm-dispatch/oracle.tsv` binds these addresses and
constants. Rust preserves the in-process searchable topic index and launches only an installed
adjacent regular CHM through one direct platform process, without bundling or altering that file.

The complete original Help-menu inventory also removes a false diagnostic gap. At the end of
`CreateMainApplicationMenu` (`00449DC0`), Lunar Magic creates one popup containing only command
`$25E4` (Contents) and `$25E5` (About Lunar Magic). `CreateOverworldEditorMenuBar` (`0054A420`)
contains only `$25E4` and `$2198` (About Overworld Editor). The retained
`help-chm-dispatch/menu.tsv` binds both two-item lists. There is no original Help-menu compatibility
diagnostics command; Scan ROM/RATS reporting belongs to independently mapped File/Options
workflows. Rust's path-free Compatibility diagnostics report is therefore a native extension, not
an unresolved interpretation of original Help behavior.

The overworld ExAnimation editor is now named through `OverworldAnimatedTilesDialogProc` at `004188d0`, including its frame-edit subclass, record commit, shift/rotate behavior, and tooltips. The following functions through `0041ab70` identify overworld submap options and both combo-based and edit-field variants of the Layer 3 graphics settings editor.

The `0041b410`–`0041e300` range identifies the analogous overworld foreground graphics editors, graphics-index list transfer, Overworld Options dialog, event reveal tile-pair editor, manual overworld sprite-command parser, and common error reporters. `OverworldOptionsDialogProc` has a recovered Win32 callback prototype. The 22-entry source/destination reveal arrays and their selected-row global are typed and named.

The `0041e3d0`–`00422260` range identifies filename/common-dialog helpers, in-place copier-header conversion, the Lunar IPS creation/application engines, ROM expansion/metadata loading, level-layout dimension tables, and the first core level-object tile renderers. IPS normal records, RLE records, reserved EOF handling, optional truncate metadata, sparse growth, header normalization, and logging behavior are annotated for clean-room reuse.

The paired system-menu commands are `$23BA` (Create IPS) and `$23BB` (Apply IPS). A live 3.63
oracle settles the coordinate ambiguity: with a 512-byte copier prefix, a logical edit at `$001000`
is encoded at physical IPS offset `$001200`. The native `ToggleSnesCopierHeader` path gives a
headerless supported ROM the recovered canonical prefix before either operation and removes that
temporary prefix again afterward. Applying to a ROM that differs from the original raises `This ROM
has already been changed!` before file selection; Cancel performs no write. Wrong IPS magic raises
`This is not an IPS file!` and likewise leaves the complete file unchanged.

The `00422330`–`00424210` range consists of fixed and lookup-driven standard-object renderers. Names currently describe proven geometry and tile-selection behavior (single cells, horizontal/vertical pairs, 2x2, 3x3, 4x4, and composite patterns); comments explicitly mark exact in-game object identities as unproven where dispatcher evidence is still pending. The `0x3800`-cell Map16 tile/flag/source arrays and `0x4080`-entry modified-cell list are typed and named.

`ConfigureLevelLayoutDimensions` at `00421690` also proves the physical split of that shared
Map16 cache. Ordinary `$1B0`-stride secondary layers start at screen 16 (`$1B00`). Modes `$05`–
`$08`, `$0A`, and `$0D` instead start at screen 14 with a `$200` stride (`$1C00`); this includes
the vertical layouts previously omitted from the Rust live-cache comparison. The function writes
those bases into both its rendering offset tables and the later cache traversal table, so they are
layout state rather than a per-object adjustment.

`LoadLevelModeConfiguration` at `00469540` bounds the stored mode before consuming any of the
mode-property tables. Values `$12` through `$1D` trigger the localized warning, clear only the low
five mode bits in legacy-header byte 1, set dirty flags `$06`, and continue as mode `$00`; the
three background-color bits remain intact. This is a mutating compatibility fallback, not an
unsupported-mode error. Rust therefore keeps exact low-level header decoding but applies this
canonicalization when native editing controllers open or semantically edit a level.

The `00424310`–`00427cc0` range continues the standard-object renderer family. It now identifies packed/conditional lookup patterns, boundary-aware 2x2 and 2x3 patterns, command-mapped and rectangular fills, and five variable-height edge/column renderers. The latter are named from proven geometry and tile families; exact vanilla SMW object identities remain explicitly marked medium-confidence pending dispatcher-table recovery.

The `00427f50`–`0042fbf0` range covers additional adaptive standard-object geometry: four-column and tapered objects, top-row/remaining-row fills, capped columns, repeated 16x6 pattern strips, alternating columns and pairs, expanding edge-bounded shapes, page-1 bordered rectangles and wedges, cyclic two-column objects, and complementary ascending/descending diagonal line and edge-pair renderers. Names and comments distinguish directly proven geometry/tile behavior from still-unproven vanilla object identities.

The `0042fe20`–`00435050` range finishes the recovered renderer cluster and exposes the standard-object definition infrastructure. `LoadStandardObjectDefinitionIndexMap` reads five ROM tables of 63 packed 24-bit handler pointers and maps them to known implementations; `g_abStandardObjectDefinitionIndices` is typed as `byte[320]` (five tileset groups by 64 slots). Shared definitions and normal, castle, rope, underground, and ghost-house overlays are initialized separately, ROM-derived substitutions are installed for the active tileset, and `DispatchStandardObjectByCommandId` dispatches command IDs `0x00`–`0x97` through the recovered jump table.

Immediately following the dispatcher, `GetStandardObjectExtendedSizeNibble` and `GetEncodedLevelObjectRecordLength` recover special-command sizing rules. `DecodeLevelObjectStreamToNodeList` parses variable-length serialized records into the editor's 0x28-byte doubly linked nodes while extracting control commands into level globals, and `SplitOversizedLevelObjectNodeForSerialization` performs the complementary node split used by `SerializeLevelObjectList`.

The decoder/serializer pair also proves the placement-field boundary used by the Rust model: the low nibbles of encoded bytes zero and one are the two tile-coordinate components, swapped for the alternate level orientation, while byte-zero bit 7 is the new-screen/advance-screen flag maintained by the screen-transition normalizer. An isolated record therefore has two orientation-neutral coordinate nibbles and an advance flag, not an honest absolute X/Y position; absolute placement also depends on preceding screen-jump controls.

`NormalizeForwardScreenJumpSequence` and `InsertLevelObjectAfterPositionAnchor` prove two command-zero control forms distinguished by parameter byte `1` or `3`. For parameter `1`, the packed target is `(command1 & 0x0F) << 8 | (command0 & 0x1F)`; parameter `3` reverses those packed halves as `(command0 & 0x1F) << 8 | (command1 & 0x0F)`. The Rust model classifies and edits these exact packed targets while leaving their eventual axis interpretation to a complete level-layout context.

The object-list lifecycle and selection subsystem through `00437540` is now named. The two layer-list tails are typed as `LevelObjectNode *`; the selected-object array is typed as `LevelObjectNode **` with an explicit count. Rendering caches each node's modified Map16 cell list, rectangle selection is implemented as the symmetric difference between unique source-object sets, and deletion/normalization helpers preserve serialized screen transitions by merging, removing, or creating screen-jump control records. Confirmed node fields include links at `0x00/0x04`, cached-cell pointer/count at `0x08/0x0C`, encoded bytes from `0x0E`, decoded screen coordinates at `0x1E`, cached packed placement at `0x20`, and a transient byte at `0x26`.

Ordered insertion helpers through `00437d80` are also recovered. They locate anchors by decoded screen position, transfer transition bits for adjacent/equal positions, allocate intervening screen-jump nodes for larger gaps, repair both doubly linked directions and layer tails, and remove obsolete trailing control records. This establishes that screen-jump records are maintained as an editor-side normalization layer around real objects rather than treated as ordinary selectable objects.

The movement/duplication block through `00438fb0` is now annotated. Pixel drags are snapped to a common valid tile displacement for every selected object; old modified-cell caches are retained long enough to redraw the union of old and new coverage; nodes are detached with transition repair and reinserted in forward or reverse order to preserve overlap semantics. A separate duplication path clones each selected 0x28-byte node, clears inherited render caches, moves the clones as a group, and reinserts them at the requested level position.

Selection commands through `00439720` now cover deletion, membership testing, cursor-cell selection behavior, and ordered movement in both linked-list directions. The two reordering functions preserve relative selected-node order and serialized screen transitions while optionally retaining old cell caches for immediate redraw; their names intentionally describe proven list direction because the corresponding menu wording has not yet been tied to strings.

Visibility-order operations through `0043a2a0` are also recovered. Lunar Magic tests whether all cells covered by a selection are already frontmost/backmost, snapshots both ownership and tile output, and repeatedly reorders until the visible ordering objective is reached or a render change establishes the boundary. Additional helpers detach/reinsert a selection in either order, compute a four-direction resize mask from neighboring selected cells and object size metadata, and reconstruct either layer list from contiguous 0x28-byte node snapshots.

Live port-8089 revalidation of `AdjustSelectedObjectDimensions` at `0043C2B0` recovered one
previously omitted size encoding. Command `$27` records whose fourth byte has mode `$C0` use the
low seven bits of byte 2 for horizontal size minus one and byte 6 for vertical size minus one,
independently supporting 1–128 tiles per axis. Byte 2's high bit still controls the optional eighth
record byte and is not part of the size. `GetSelectedObjectResizeDirectionMask` at `0043A000`
confirms separate horizontal/vertical capability masks, while `SerializeLevelObjectList` at
`00435650` confirms the seven/eight-byte framing. The Rust record model and native Layer 1/Layer 2
forms now expose these two fields without disturbing mode flags or unrelated extension bytes.
The complete live gesture chain also fixes their canvas interpretation: `UpdateSelectedObjectResizeDrag`
at `004880A0` passes physical mouse X/Y deltas through `ResizeSelectionFromDragHandles` at
`0043C560` to the first/second fields of `AdjustSelectedObjectDimensions` without consulting or
swapping for vertical level mode. The Rust canvas therefore uses a physical bottom-right handle,
retaining horizontal/vertical field identity in both level orientations.

Rendering was separately traced through `RenderLevelObjectNodeListAndCacheCells` (`00435CF0`),
`InstallStandardObjectDefinitionsForTileset` (`00433BC0`), and
`DispatchStandardObjectByCommandId` (`00433F90`). The command-`$27` mode-`$C0` extension fields are
used for physical selection/resize geometry; they do not replace the active tileset family's
standard-object renderer or its ordinary parameter semantics. The Rust editor therefore uses the
fields for interaction bounds while continuing to dispatch artwork through the authenticated
handler map.

The custom object clipboard and template-placement subsystem through `0043b100` is now named. `LevelObjectClipboardHeader` is a recovered 32-byte structure followed by fixed 0x28-byte node records in registered format `Lunar Magic Objects V6`; copy computes selection-origin and rendered-margin metadata, while paste validates format/version fields, filters incompatible extended objects, converts encodings, translates clones, and reinserts them. The same node-stream decoder supports temporary multi-object templates, isolated preview rendering with full live-Map16 save/restore, and placement of cloned template groups. Screen-exit object evidence also upgraded the earlier generic per-screen control-node interpretation: dedicated helpers now deduplicate, create/delete, normalize flags, and synchronize the 32-entry packed screen-exit array.

The object-edit pipeline through `0043cb20` is now named and commented. It clones selections into template lists, converts connected Map16 regions into rectangular Direct Map16 object commands, performs handle-driven resizing with transition-safe detach/reinsert ordering, and rebuilds/redraws cached modified-cell regions. Three helpers isolate the packed properties and 15-bit reference-remapping behavior of encoded extended command ID `0x27`; the bit packing is established, while the user-facing meaning of that referenced resource remains intentionally marked medium confidence.

The following block now covers object-reference remapping scripts, manual-command insertion, invalid screen-exit diagnostics, legacy object-stream filtering, and the beginning of the Direct3D 9 renderer. The remapping language uses a 0x8000-entry translation table with replacement, signed-offset, sequential, and 16x16-grid modes. Renderer helpers dynamically resolve `Direct3DCreate9`, partition large render extents into device-compatible tiled surfaces, build four textured vertices per tile, and release COM resources along each failure and shutdown path.

The Direct3D and Windows compatibility range through `00440f90` is now classified. It includes complete renderer context lifecycle and tiled presentation, lazy USER32 multi-monitor API resolution with a single-monitor fallback, external ROM/GFX editor command-template expansion and process launch, executable-path startup validation, and CHM help-file opening with `Zone.Identifier` cleanup.

Rust now exposes the safe process boundary of that external-GFX path in the installed graphics
editor. The active staged slot is written under its canonical public filename in a private
create-new directory. Persisted portable tools containing `{graphics}` can be selected and expand
the private path plus ordinary project context only after staging; eligibility requires that value
in a direct argument rather than the process working directory. The executable and all direct
arguments are approved explicitly, and a
background worker waits for completion. Only a successful, exact-size, nonsymlink regular file is
reloaded, through the revision-bound graphics controller; every terminal path removes the private
workspace. Existing macOS `.app` paths use the shell-free system launcher with explicit wait,
new-instance, application, and argument boundaries. Exact Lunar Magic command-template syntax
remains separate recovery work.

The ROM-address and level-coordinate utilities through `00441fe0` are now named and annotated. High-confidence helpers implement both directions of SNES/PC address conversion for the detected mapping mode, horizontal/vertical level-layout cell indexing, status/scrollbar initialization, and packed ROM-word access. Several register-convention stream wrappers are intentionally given structural names and medium-confidence comments until their hidden value widths can be proved from disassembly and all callers; the ExLoROM Work RAM bank-byte validator is separately identified by its required `0x7E`/`0x7F` values.

The range through `004431c0` now identifies expanded-ROM relocation validation, level dirty-state propagation, level-state teardown, packed level-header setters, writable-ROM-range checks, screen-count derivation, common file dialogs, auxiliary editor window lifecycle, and DPI-aware icon installation. The four compact level-header writers document exact byte and bit positions but deliberately leave the UI field names unresolved until the associated dialog controls or format tables prove whether each field is a palette, tileset, or other selector. Relocation helpers separately validate IRAM word/byte ranges and Work RAM bank bytes before altering ROM data.

Fresh decompiles of `RenderLevelObjectNodeListAndCacheCells` at `00435CF0`,
`FindLastScreenContainingRenderedObjectCells` at `0043D5B0`, and
`UpdateLevelScreenCountFromContent` at `00442600` close the packed screen-jump boundary. The
renderer keeps separate one-byte primary and secondary cursors. A low-first horizontal jump maps
them through the mode-0 `$1B0` screen-offset table plus a `$200` secondary stride; a high-first
vertical jump uses equal `$200` strides. An ordinary record's high bit increments the primary
cursor, which is masked to five bits before cell placement. Automatic extent is then derived from
the last screen containing a nonzero rendered source-object cell, combined with sprite extent and
clamped to the active mode's screen count. This explains the live maximum low-first `$1F/$0F`
case: without an advance its cell is outside the 32-screen plane, while one advance wraps the
primary cursor and maps the secondary `$0F` row to screen `$11`.

The Windows DPI compatibility block through `004436f0` is now fully named. It dynamically resolves per-monitor DPI APIs with older-system fallbacks, installs appropriately sized small and large icon resources, controls dialog scaling behavior, reads the configured cursor base size, measures the visible cursor image and hotspot, and calculates a monitor-contained popup position that avoids obscuring the pointer.

The tracking-tooltip and custom sidecar-metadata subsystem through `00445f90` is now named and typed. `ExternalMetadataGroupAEntry` is a recovered 12-byte record containing three owned pointers; `ExternalMetadataGroupBEntry` is a 28-byte record with three owned pointers plus 16 bytes of fields whose individual meanings remain unresolved. Their 1024-entry and 832-entry pointer/count/dimension/flag arrays now have explicit global names and array types. The `.msc`, `.ssc`, and `.osc` loaders are separated: `.msc` supplies two 256-entry label tables, `.ssc` parses custom-sprite display/tooltip/remap data into group A, and `.osc` parses custom-object display/tooltip/attribute data into group B. Allocation helpers document the compact nibble-to-dimension mapping and preserve both one-dimensional and two-dimensional storage modes.

The `.ssc` loader at `00444e50` is now represented by a lossless Rust source model. It
authenticates the two hexadecimal selector fields, extra-bit-derived 1024-entry index, compact
dimensions or clamped 3–15-byte record length, alternate/global flags, escaped descriptions,
explicit display triples, `*text*` glyph expansion, four-word palette records, and both global
range-remap forms. Display records feed the same recovered preview-definition table as standard
sprites; malformed lines remain in the preserved source but do not materialize metadata.

The corresponding `.osc` loader at `00445f90` now has a lossless Rust model and resolved
source-order overlay. Its three hexadecimal header fields select an object family, parameter, and
flags; ordinary records either target one of five object-definition variants or expand across all
five, while object types `$00` and `$2D` use the recovered `$140` and `$240` special index ranges.
The model preserves compact dimensions, clamped 2–15-byte linear records, alternate storage, escaped
descriptions, signed display triples, eight-word value records, and up to fifteen attribute bytes.
Resolved display records feed a dedicated custom-object preview boundary.

The custom sidecar pipeline is now connected through `.dsc`, `.m16`, and `.s16` loading. The 32768-entry description-pointer/flag/mapping tables and raw 0x2000-byte `.m16` plus 0x1C000-byte `.s16` buffers have explicit names and array types. Initialization establishes the default custom display/remap sentinels before overrides. The paired SNES BGR555/RGB conversion functions are named separately, including their alternate quantization behavior.

The `.dsc` reader at `LoadAllCustomSidecarMetadata` has now been translated into a focused Rust
lossless-source model and a separate resolved table. It accepts an optional UTF-8 BOM, parses
tab-separated hexadecimal key/flag pairs below `0x8000`, expands bit-0 descriptions across a
256-entry page, masks mapping values to 15 bits, rejects alternate mappings at or above `0x3D00`,
and applies records in source order. Disassembly of the jump table at `00447000` proves `\\`,
`\n`, `\r`, and four `%06X` style escapes selected by `\b`, `\d`, `\f`, and `\m`; unknown escapes
become spaces. No `.dsc` writer was found in the binary, so the application uses validated,
revisioned whole-source replacement and exact-byte persistence rather than inventing one.

`RenderMap16TileToPixelBuffer` confirms that direct `.dsc` mappings are render-time substitutions:
native flag bit 2 selects the mapping under the first feature switch unless its suppressor is set,
flag bit 4 selects it under the second feature switch, and either path enables averaged blending.
Flag bit 1 enables blending without substitution. Built-in IDs `0x21`/`0x22`, `0x23`, and `0x24`
map to `0x114`, `0x113`, and `0x115` under the second switch; `0x27` through `0x2A` select blending
under the unsuppressed first switch. The Rust display resolver and portable Map16 renderer now model
this boundary separately from alternate mappings consumed by `BuildMap16CustomDisplayMappings`.

The alternate-mapping pass is now recovered independently. It resolves Acts Like chains only when
the immediate target exceeds `0x1FF`, consults alternate `.dsc` mappings by the resulting lookup
key, preserves native `0x4000` and `0x8000` control bits, and emits the parallel per-cell `0x20`
marker flag. Its built-in substitutions include the position-dependent `0x111`, `0x11A`, `0x11D`,
and `0x125` tables read at `005B4874`, plus the mode-dependent `0x16A`/`0x16B` cases. The Rust
materializer returns both complete buffers atomically and exposes sparse oracle observations.

The main toolbar rendering and tooltip block through `0044a7f0` is now named. It retrieves command rectangles, custom-paints the current hexadecimal level number over the Open Level button, draws toolbar group separators, subclasses ANSI/Unicode toolbar painting, synchronizes layer/sprite editing-mode buttons, and supplies localized or fallback help text for toolbar commands.

The main toolbar/rebar/status-bar construction and scaling block through `0044cdc0` is now named. It creates all 52 toolbar button records and their command mappings, loads external `.ff4`/`.ffxi` visual assets with compressed-resource and legacy fallbacks, creates and updates rebar bands, hides overflowing button groups, paints custom separators, and owns toolbar image-list resources. Status-bar helpers create and measure parts across monitor DPIs. Monitor work-area queries and DPI rounding/scaling functions are separated, and toolbar bitmap strips are converted to 32-bit DIBs, recolored to the current system button face, and scaled icon-by-icon.

The editor render-surface, ROM-open/restore, and core tile-rendering range through `0044fab0` is now named. The primary level-editor DIB is resized with scrollbar/origin reconciliation and allocation fallback, while a separate shared 512x512 surface and external `.ffx` overlay surface have explicit lifecycle functions. ROM helpers identify copier-header detection/insertion, file timestamp capture, editing-mode initialization, and full restore-point creation. Rendering helpers now distinguish a single Map16 tile, linked `.m16` custom display objects, and Layer 3 tilemap regions; their comments enumerate palette, flip, priority, transparency, custom remap, inversion, darkening, averaging, and additive-blending behavior.

The level-editor overlay and viewport-composition block through `004530a0` is now named. Low-level primitives cover clipped saturating color addition, blended double outlines, selection borders, dashed rectangles, Map16 grid lines, and clipped text with translucent background preservation. Higher-level passes draw logical screen boundaries and labels, screen-exit destinations, primary/midway and secondary entrance labels, level-mode boundary guides, and invalid Map16-cell warnings. `RenderLevelEditorViewportRegion` is identified as the main dirty-rectangle compositor: it initializes background pixels, renders the enabled level layers and sidecar graphics, and applies all configured editor overlays. Its `g_abLevelMap16CellFlags & $60` redraw is specifically bracketed by the LMSW viewport-overlay backing capture/restore and is not a general high-priority-over-standard-sprites pass. The object-backed Layer 2 branch sets `DAT_00600256` only when object tileset 3 is active; the Map16 renderer consequently redirects encoded palette rows 0–3 to CGRAM rows 4–7 while leaving encoded rows 4–7 unchanged.

The full-level image export and initial SNES graphics-decoding block through `00455040` is now named. Export helpers calculate complete level dimensions, replace the interactive render surface temporarily, render the level in 16-pixel strips, and emit either bottom-up 24-bit BMP data or a packed RGB buffer passed to the PNG encoder before restoring editor state. The adjacent planar decoders expand SNES 8x8 tiles into indexed pixels, with a specialized 4-bpp implementation and a generic 1-through-8-bpp implementation.

A pristine-ROM corpus run now exercises that native path directly under Wine.
With modified-only and auto-screen adjustment both disabled, Lunar Magic 3.63
exports 488 complete-level PNGs using stored extents and declines 24 empty
Layer 1 slots (`095,096,097,098,099,09A,09B,0CC,0D5,0D9,0DF,0E2,0E5,195,
196,197,198,199,19A,19B,1C7,1DE,1EB,1F6`). This establishes the full-corpus
oracle contract: Rust must match 488 rendered outcomes plus those same 24
native non-renderable outcomes; treating the latter as successful blank images
would be a parity failure.

The reciprocal SNES 4-bpp encoder, level-editor activation/teardown, and Map16 persistence range through `00458f90` is now named. UI helpers finalize a loaded level at the selected entrance, enable or disable the complete editing command set, guard destructive operations with modified-level/shared-palette/auxiliary-editor prompts, and release all render surfaces when a ROM closes. The Map16 pipeline now distinguishes base data, the primary remap/page allocation transaction, and eight secondary page blocks; separate legacy/expanded loaders and savers resolve mapper-specific pointers, trim empty `0x1004` tails, repair invalid or cyclic remap chains, retire old allocations, and patch relocated block pointers.

Whole-ROM Map16 bank import/export and the ExAnimation runtime through `0045b360` are now named. The Map16 helpers iterate all fifteen FG/BG banks while preserving the active editor bank. ExAnimation helpers detect legacy versus expanded formats, advance vanilla/global/level animation records, implement trigger and frame sequencing, transfer planar graphics and palette data, maintain destination-ownership maps, run the preview timer, and navigate between clicked tiles or colors and their owning animation records. Project `Graphics` and `ExternalGraphics` file readers, external palette conversion, and loading/validation of animated GFX 33 plus player GFX 32 are also identified.

The graphics/ExAnimation patch installer and ExAnimation serialization range through `0045f550` is now named. External sprite graphics and palettes are loaded into decoded caches. Bulk GFX insertion/extraction helpers identify legacy and expanded table markers, allocate runtime blocks, install pointer-table and IRAM relocations, normalize older tables, and install the expanded ExAnimation runtime with mapper-specific address validation. The data layer now has explicit legacy-record conversion, compact serialization/deserialization, level-table pointer resolution, block validation, trigger-state reconciliation, runtime reset, and level-data clearing functions.

Global ExAnimation persistence, feature-control patches, and the expanded per-level header/settings system through `00462000` are now named. The global animation transaction migrates as many as 512 legacy blocks into compact expanded records, supports lazy load/save, and explicitly swaps persistent global or level arrays into the shared editor working set. Feature-control helpers encode four enable/disable bits and install a relocatable table/runtime. The following subsystem allocates a 0x6E00-byte expanded level-settings runtime/table, converts legacy parallel tables into 512 fixed 0x20-byte records, upgrades older layouts, normalizes 12-bit references and sentinels, and patches mapper-specific hooks. Ghidra now contains an `ExpandedLevelHeaderRecord` structure with sixteen 16-bit offset-named fields; semantic field names remain pending evidence from its UI consumers.

Expanded level-settings integration, the Layer 3 main patch, and GFX pointer access through `00463db0` are now named. Current-level and overworld save paths detect, install, or migrate the expanded settings table; normalize individual records; and install Layer 3 support only when high record flags require it. The localized `Layer 3 main patch` transaction validates and relocates a 0x4C0-byte payload and upgrades older hook signatures. Additional helpers install two level-save support patches, identify the three ROM-open markers controlling a one-byte stream bias (exact purpose still unresolved), resolve and write GFX/ExGFX ROM pointers across vanilla and expanded slot ranges, and load/decompress graphics with explicit missing/inaccessible/oversize diagnostics.

The level graphics-set assembly and Layer 3 tilemap pipeline through `004653b0` are now named. Separate loaders resolve FG/BG, sprite, Layer 3, Special World, and individually selected graphics from vanilla tables, ROM-resident expanded selections, or the current expanded level record. High nibbles for eight 12-bit FG/BG or sprite slot IDs are packed/unpacked explicitly. Layer 3 helpers derive mode/offset flags, decode tilemap graphics ranges, parse literal/repeated remap command streams, load the selected tilemap data, and reset its 0x2000-byte buffer. The cache finalizer decodes all planar graphics and establishes the initial ExAnimation frame, while lookup initialization identifies vanilla animated-tile ownership and trigger indices.

Map16 custom-display mapping, the Layer 2 object-data migration pipeline, and shared palette file support through `00467ce0` are now named. The Map16 pass resolves built-in and `.dsc` substitutions plus conditional display flags over all 0x3800 cells. Layer 2 helpers detect four table generations, install the current runtime, migrate and validate 512 pointers, resolve/decompress a selected level stream, and convert legacy expanded or interleaved tilemap layouts. Shared palette helpers install two table hooks and a 0x600-byte runtime, while `.smwpal` import/export handles backend-specific sizes and commits imported colors to the ROM.

Full-palette persistence, palette editor cache construction, palette preview/snapshot generation, graphics-file insertion, level-mode setup, layout validation, and the first VRAM runtime installer block through `00469960` are now named and commented. The palette routines distinguish legacy and expanded `.smwpal` layouts, convert the 256-entry SNES BGR555 palette into RGB display caches, preserve global state while generating arbitrary-level previews, and refresh dependent editor windows. The VRAM installer selects feature-specific embedded resources, allocates ROM space, applies relocations, installs hooks, and records the selected runtime version.

Verified symbol coverage after this pass: **1,185 named functions out of 3,912 total; 2,727 autogenerated `FUN_...` symbols remain.**

Save-time runtime compatibility and installation helpers through `0046d8ff` are now substantially resolved. The named functions cover VRAM runtime version detection/replacement, auxiliary relocated runtimes, mapper-aware table-bank pointer commits, four-way layer support, the initialized 0x200-entry level-data pointer table, object-length override loading, level-mode tile-capacity defaults, and the mapper/feature-specific Layer 3 main runtime installer. Comments explicitly record confidence and preserve provisional feature identities where only the operation—not the owning subsystem—is yet proven.

Verified symbol coverage after the subsequent runtime pass: **1,215 named functions out of 3,912 total; 2,697 autogenerated `FUN_...` symbols remain.**

Layer 3 migration and Map16 pointer-runtime helpers through `0046fd30` are now named. The Layer 3 group detects and replaces legacy Lunar Magic runtime headers, installs mapper-specific main/extended resources and compatibility bridges, and applies embedded hook and IRAM relocations. The Map16 group resolves two families of packed block pointers into PC offsets, rewrites their split pointer words, installs the primary pointer runtime, repairs multiple legacy hook signatures, and manages auxiliary table pointers. Functions whose comparisons were removed by Ghidra's unreachable-code simplification are explicitly marked low-confidence at the exact-signature level rather than assigned invented semantics.

Follow-up live decompilation confirmed the installed expanded per-level settings table uses exactly
512 fixed 0x20-byte records. `InitializeDefaultExpandedLevelHeaderRecord`, the two record migration
helpers, and `EnsureCurrentLevelSettingsRuntime` establish the lossless record boundary and show
that the selected record is written before optional Layer 3 runtime installation. The Rust model now
loads and saves an explicitly located installed table without normalizing its sixteen little-endian
words. This does not claim clean-ROM runtime installation: its descriptor-indexed hooks,
relocations, embedded 0x4C0-byte patch, and mapper variants remain a separate verified-patch task.

Verified symbol coverage after this pass: **1,241 named functions out of 3,912 total; 2,671 autogenerated `FUN_...` symbols remain.**

The remaining staged Map16 hook installers and the adjacent sprite-19/Lfix3 patch family through `00471d50` are now named. The Map16 path now exposes four compatibility stages, auxiliary table allocation, final CDM16 repair, and mapper-specific compatibility upgrades. Embedded UI and patch labels prove the next cluster: it prompts for and installs Lunar Magic's sprite 19 ASM fix, recognizes Lfix3 runtime versions, allocates the current 0x510-byte Lfix3 payload, initializes three 512-entry runtime tables, and migrates two legacy Lfix3 table layouts. Later instruction-level review corrected the initially misleading legacy-probe name: its unsigned compare accepts marker version `$0111` and newer, while the two older generations are selected by separate JSL hooks. Low-confidence names remain restricted to decompiler-elided signature probes and unidentified post-Lfix3 hooks.

Verified symbol coverage after this pass: **1,273 named functions out of 3,912 total; 2,639 autogenerated `FUN_...` symbols remain.**

Expanded secondary-exit support through `00473d30` is now named and structurally documented. The binary maintains six parallel 0x2000-byte planes for destination low, position/method, screen/Y, destination-high flags, X/overworld flags, and additional flags. The named functions detect legacy/current formats, allocate the current 0xD0-byte runtime plus four 0x200-byte tables, migrate packed flag bits, load all tables, locate/free/delete entries, and coordinate upgrades with Lfix3. A logical six-byte `SecondaryExitRecord` structure was added for clean-room reimplementation; comments note that the executable uses structure-of-arrays storage rather than interleaved records.

Verified symbol coverage after this pass: **1,302 named functions out of 3,912 total; 2,610 autogenerated `FUN_...` symbols remain.**

Secondary-exit serialization, the top-level ROM level loader/new-level initializer, known vanilla level-data fixups, and general palette file I/O through `004768c0` are now named. `SaveAllSecondaryExitTables` trims unused tails, preserves compact in-place planes, allocates variable-length planes, and updates mapped pointers; Save Level As retargets all incoming exits. `LoadLevelFromRom` now exposes the complete editor transaction from extension-table loading and mapper pointer conversion through object/sprite rebuild and redraw. Palette support distinguishes RGB `.pal`, versioned TPL, ZSNES/emulator state formats, raw SNES colors, and `.palmask` selection masks, including byte-order inference and BGR555 conversion.

`ImportFullPaletteFile` and `ExportFullPaletteFile` establish the complete `.smwpal` layouts used by the clean-room model. The legacy palette backend transfers exactly `0x7E2` bytes. The expanded backend transfers `0x800` bytes from the main working-palette region followed by a distinct `0x10`-byte auxiliary region (located immediately before the main region in memory but appended after it in the file), for an exact `0x810`-byte artifact.

`LoadPaletteFromSupportedFile` and `SavePaletteToSupportedFile` prove the native TPL version-2 framing independently: ASCII `TPL`, one version byte equal to `2`, then exactly `0x200` bytes containing 256 little-endian SNES BGR555 words. TPL version `0` instead contains RGB triplets and remains a separately interpreted variant rather than being accepted by the native-word decoder.

The level editor dispatches `PromptAndSavePaletteFile` and `PromptAndLoadPaletteFile` through
commands `$239F` and `$23A0`. A retained isolated-Wine run proves these are the per-level transfer
commands, distinct from palette-editor buttons `$2264/$2265`, which own complete shared `.smwpal`
files. Per-level export selects its encoding by extension: `.pal` writes `$300` RGB bytes, `.tpl`
writes `TPL` version 2 plus `$200` native bytes, and other accepted extensions such as `.mw3`
write all `$101` little-endian working words. Before supported 256-color export, every row-zero
entry is replaced by the backdrop word. Import automatically derives the same-basename
`.palmask`, initializes a missing selector to 257 enabled bytes, applies only selected entries,
clears selected row-zero colors, and auto-enables the level custom palette through
`CommitLevelObjectPaletteEdit`. An invalid TPL version preserves the working colors but leaves the
transient selector reset to all enabled; Rust intentionally retains the stronger failure-atomic
selector behavior.

The same dispatcher proves the extension-independent raw palette as exactly `0x202` bytes, or 257 little-endian SNES colors. Its optional same-basename `.palmask` sibling is exactly `0x101` selector bytes: a zero retains the working color and any nonzero value imports the corresponding source. The bundled 3.63 help and executable strings confirm the full extension and automatic sibling discovery. After import, selected first colors of rows 0–15 (indices `0x00`, `0x10`, …, `0xF0`) are forced to zero; the separate color at index `0x100` is not part of that clearing loop.

RGB `.pal` files are exactly `0x300` bytes: 256 ordered red/green/blue triplets. `DetectPaletteRgbByteOrdering` is more precisely an expansion detector. For enabled colors, it counts evidence with any low-three channel bits and separately counts triplets whose low bits are all zero but whose high three bits are nonzero; a strict majority of the latter selects high-bits-only `xxxxx000`, otherwise five-bit values use replicated low bits. The conversion routine chooses the nearest replicated value for noncanonical inputs, preferring the higher five-bit level on an exact distance tie.

Verified symbol coverage after this pass: **1,317 named functions out of 3,912 total; 2,595 autogenerated `FUN_...` symbols remain.**

MWL level-file import/export and recent-file UI support through `004797d0` are now named. The importer auto-detects binary `LM` containers and legacy text manifests, validates versioned section offsets/sizes, upgrades historical headers and ExAnimation records, imports packed secondary exits, and converts stored SNES addresses for each mapper. The binary exporter writes MWL version `0x0363` with an eight-entry section directory covering level header, Layer 1, Layer 2, sprites, palette, secondary exits, ExAnimation, and the expanded header. Legacy export writes the text manifest plus `.mw0`-`.mw3` sidecars. Recent-file helpers manage ten paths, UTF-8-safe abbreviated menu labels, insertion/removal, and persistent menu rebuilding.

Live port-8089 revalidation of `ImportLevelFileAutoDetect` at `00477940` also recovers two
non-obvious legacy-version rules. Auto-detection requires the `Lunar Magic ` signature at the
physical start of the file; leading comments are not skipped. The importer initializes the
version to `$0132`, then replaces it only when both the fixed-position `%1X` major and `%2X` minor
scans succeed. Thus malformed version text defaults to the six-field 1.32 layout, while parseable
future versions (for example 9.99) are accepted through the current layout rather than rejected.
The `$0132` boundary controls the five-versus-six level-header fields and eight-versus-twelve-bit
secondary-exit indexes; `$0341` controls the recovered Layer 2 flag normalization. Rust mirrors
these permissive compatibility branches while retaining bounded lines, safe sidecar names, and
exact required manifest/sidecar field validation.

The same function's legacy text loop is intentionally not failure-atomic. Its `%03X` source-level
field is clamped to the last available editor slot. Each secondary-exit row is parsed independently:
a malformed row raises the original error prompt but import continues, and a later valid row for an
already-written index replaces the earlier table value. Rust's `decode_with_diagnostics` models
that final state without unsafe `fscanf` continuation: it emits structured clamp/ignored/replaced
diagnostics, retains all bounded structural failures as hard errors, and the native two-stage
import displays the compatibility summary before it prepares a ROM transaction. Programmatically
constructed manifests remain strictly validated so callers cannot accidentally publish duplicate
keys; only file decoding applies the original recovery policy.

Two format structures were added: `MwlSectionDirectoryEntry` (8 bytes: file offset and byte length) and `MwlSecondaryExitEntry` (8 bytes: 16-bit exit index, five semantic field bytes, and one reserved byte).

The recovered binary import loop caps the section at `$10000` bytes (8,192 records), writes a record
only when its index is below `$2000`, applies records in file order so the last duplicate wins, and
never reads byte 7. This differs from the legacy text importer's explicit bad-index error. Rust keeps
standalone binary MWL parsing lossless, then applies the binary importer's cap, skip, duplicate, and
reserved-byte rules at the installed-ROM overlay boundary. A reciprocal live oracle covers `$0000`,
`$1FFF`, `$2000`, duplicate keys, nonzero reserved bytes, every packed-field maximum, installed-table
reopen, empty-set clear, and final checksum validity.

Verified symbol coverage after this pass: **1,335 named functions out of 3,912 total; 2,577 autogenerated `FUN_...` symbols remain.**

MWL save orchestration and the level-editor undo/redo core through `0047b320` are now named. The save wrapper serializes Layer 1/2 and sprites before selecting binary or legacy export and updating recent files. Undo history uses fixed 0x28-byte doubly linked nodes with ownership/change flags; snapshots may share unchanged layer payloads and optionally include a 0xC00E-byte extended block containing fourteen header bytes plus all six 0x2000-byte secondary-exit planes. Capture, restore, pruning, allocation failure, reset, history-limit configuration, and menu-state updates are labeled. `ConfigureLevelUndoHistoryLimit` at `0047A6E0` and `SetOverworldUndoHistoryLimit` at `00540340` consume the same persisted `UndoMain` setting: its initialized value is 33, its UI/storage range is 0 through 51, and the count includes the current baseline, leaving at most one fewer undoable operations. Capture returns without allocating when the value is below two, and transitions involving either disabled value rebuild the history chain. The Rust application reproduces those limits for the shared project history and persists them through the native Tools dialog; the remaining oracle gap is interactive persistence and per-editor transaction grouping in the original. Adjacent helpers for copying a background from another level, reloading object/graphics/Layer 3 resources, finalizing edit transactions, and validated/fast redraws are also named.

The `LevelUndoRecord` structure was added at 0x28 bytes with flags, four snapshot pointers, three metadata words, and next/previous pointers.

Verified symbol coverage after this pass: **1,353 named functions out of 3,912 total; 2,559 autogenerated `FUN_...` symbols remain.**

Level-editor redraw dispatch, ROM-layout descriptor conversion, checksum compensation, Lunar Magic version detection, and the top-level ROM-open transaction through `0047d230` are now named. The ROM validator recognizes base SMW revisions and All-Stars+World, detects copier headers and LoROM/ExLoROM/SA-1 mapping, selects or converts the 0xC10-byte layout descriptor, verifies checksum/version compatibility, decodes installed runtime metadata, and initializes feature state. `ProcessRomImageOpenTransaction` then loads sidecars, Map16, secondary exits, ExAnimation, metadata, graphics tables, and the active level. The checksum routines implement mirrored SNES checksum accumulation for non-power-of-two images and write a compensation block when needed. Rust's retained identity corpus now exercises all 48 accepted game/region/map-mode/header/checksum combinations and rejects the unsupported All-Stars/Japan pairing. A second opt-in gate classifies twelve checksum-valid Lunar Magic 3.63 modified-ROM outputs spanning level, overworld, ExAnimation, palette, title, credits, Layer 3, and optional-asset changes, plus headerless and checksum-damaged forms.

The close/open guard is a save-prompt chain, not a binary discard guard.
`CheckCanProceedAfterCoreSavePrompts` (`00455F50`) calls the modified-level, shared-palette, and
final unsaved-state prompts; `CheckCanProceedAfterAllSavePrompts` (`00455F80`) prepends the active
overworld-mode prompt. `PromptToSaveModifiedLevel` (`00491080`) treats result 2 as cancellation,
result 6 as Yes and synchronously dispatches command `$2392`, and then clears the modified marker
for every non-cancel result. The retained Wine dialog is titled `Lunar Magic`, asks
`Save level to ROM?`, and exposes Yes/No/Cancel as control IDs 6/7/2. Cancel retained both the
frame and modified byte; No closed the process. The native application consequently defers close,
quit, or replacement-open until its asynchronous save has been acknowledged, while cancellation
or any failed/stale persistence leaves the dirty project open.
The native variant regression executes that persistence boundary for all 48 accepted combinations:
three game/region identities, four map modes, both copier-header states, and both valid and damaged
checksums. Each case preserves its complete physical prefix and closes only after the saved snapshot
is acknowledged.

Lunar Magic keeps the selected ROM as a writable backing stream rather than exposing a ROM
Save As command. `OpenRomBackingStream` (`004A69E0`) requests read/write access and selects either
the file handle or a growable memory image; `WriteRomStream` (`004A6C10`) updates that shared image
or file at the tracked cursor; `CloseRomBackingStream` (`004A6AA0`) flushes the memory image,
closes the handle, and commits its restore point. The Rust application additionally offers
transactional Save As. An approved existing regular destination now routes through atomic
replacement and a distinct acknowledgement that adopts the new path; an absent destination stays
strictly create-new, and symlinks/non-files reject. The complete 48-identity product is exercised
in both modes, for 96 actual publications with exact physical-byte comparison and subsequent-Save
target verification.

The retained live save closes the original success boundary. From the exact headered pristine
input, Yes at `Save level to ROM?` opened `Save Level to ROM as (in hex)`; accepting its default
expansion produced a 1,049,088-byte physical image (1 MiB logical), retained the copier header,
installed 13 RATS owners, passed the SNES checksum verifier, and closed the process. Its exact
SHA-256 is `69cc6693ccd83f67369479314466b53c50e57569d319d9f8078667cfc025928e`.
The separate retained Cancel gesture leaves the frame and modified byte unchanged. Since the
original has no whole-ROM destination chooser, there is no original Save As collision gesture;
that failure surface belongs to Rust's stronger transactional publication workflow.

Verified symbol coverage after this pass: **1,371 named functions out of 3,912 total; 2,541 autogenerated `FUN_...` symbols remain.**

ROM expansion/metadata and graphics-compression management through `00480760` are now named. Lunar Magic's metadata writer emits the public-version identification/attribution block, packed mapper and feature flags, compression configuration, runtime pointers, VRAM version, and optional checksum compensation. Graphics helpers resolve AllGFX offsets, recognize stock signature pairs, synthesize a missing fourth SNES bitplane for compatible assets, erase standard/auxiliary/ExGFX allocations, and report insertion failures. The compression-mode transaction extracts graphics when formats are incompatible, replaces mapper-specific mode-1/mode-2 runtime resources, converts dependent tables, reinserts graphics, updates metadata, and cleans its temporary directories. The ExAnimation `Bypass.lst` exporter is also labeled.

The top-level expansion command branches call `ExpandRomToRequestedSize` with physical targets
`$200200`, `$300200`, `$400200`, `$600200`, and `$800200`. The first three are the ordinary
2/3/4 MiB commands. Retained message text and the handler's SA-1 flag prove that 6/8 MiB are
SA-1-only commands, not ExLoROM choices. `ConvertRomTo64MbitExLoROM` is a separate confirmed
LoROM-to-8-MiB transaction with its own compatibility prompt and prerequisites. At `$004A7390`,
`ExpandRomBackingStore` masks the physical request to its supported boundary, refuses shrinking,
and grows through `ZeroRomRange`; therefore newly expanded bytes are `$00`, not `$FF`. It also
installs or removes the mapper-specific ZSNES compatibility locks after SA-1 growth. The native
dialog defaults pristine/ordinary ROMs to the same next fixed 2/3/4 MiB target and `$00` fill while
retaining its explicit advanced hexadecimal route.

The separate ExLoROM transaction is now recovered and implemented. For sources through 4 MiB it
copies the first 3.5 MiB to `$400000`, retains the first low bank and any source bytes in
`$380000..$400000`, clears the other low banks, and grows the physical image to 8 MiB. It writes
the 3.63 attribution at `$47F0A0`, the 26-byte conversion record at `$47FFE6`, changes both ROM-size
bytes to `$0D`, and installs two full-bank `STAR` locks at `$7F0000` and `$7F8000` with the recovered
`ExLoROM NULL bank lock` payload. The `$47F000..$47F0A0` compensation area preserves the source
checksum exactly. A pristine live-Wine oracle is byte-identical, and a patterned 4 MiB oracle proves
the split relocation/retention boundary. Rust performs this as one failure-atomic mapper-aware
history operation and requalifies identity on conversion, undo, redo, and save/reopen.

The SA-1 expansion path is also recovered and implemented byte-for-byte. Physical offsets
`$000000..$1FFFFF`, `$200000..$3FFFFF`, and `$400000..$7FFFFF` canonically map through SNES banks
`$00..$3F`, `$80..$BF`, and `$C0..$FF`, respectively. At 6 MiB Lunar Magic installs lock payloads
of `$7FB0`, `$7FF8`, and `$FFF8` bytes at `$400000`, `$408000`, and `$410000`, each beginning with
the recovered ZSNES 1.51 warning. A fourth `$40`-byte RATS payload at `$407FB8` mirrors the internal
header and remains usable between the locks. Expansion from 6 to 8 MiB clears exactly the three
lock allocations, retains the mirrored-header allocation, and zero-fills the new tail. Both forms
write the `$0D` size byte and SA-1 metadata record while using `$7F000..$7F0A0` compensation to
preserve the stored checksum. Direct 6 MiB and subsequent 6→8 MiB Wine oracles match the Rust
outputs byte-for-byte; application tests cover save/reopen and independent undo/redo.

Verified symbol coverage after this pass: **1,389 named functions out of 3,912 total; 2,523 autogenerated `FUN_...` symbols remain.**

Legacy ExGFX bypass import, Layer 3 tilemap GFX writing, level-mode Layer 2 classification, and the five per-level payload pointer-table loaders through `004814a0` are now named. `ImportLegacyExGfxBypassList` installs ExAnimation feature control support when necessary and commits the 0x400-byte `Bypass.lst` table. The pointer loaders read up to 0x209 three-byte entries for Layer 1 objects, Layer 2, sprites, optional palettes, and ExAnimation, handle multiple installed table generations/sentinels, convert SNES addresses to mapper-specific PC offsets, and zero-fill unused indices.

Fresh decompiles of `LoadExpandedHeaderForegroundBackgroundGraphicsSet` at `00464560` and
`LoadExpandedHeaderSpriteGraphicsSet` at `00464670` recover the Super GFX bypass mapping inside the
16-word expanded level header. Word 0 bit 15 selects the expanded loaders; six FG/BG 12-bit file
numbers occupy reversed words 7 through 2, and four sprite file numbers occupy reversed words 11
through 8. Rust now exposes a typed, range-checked model that preserves every unknown high bit and
unrelated word. The advanced level editor presents the six FG/BG and four SP fields while retaining
the complete raw-word editor beneath them. The installed native-level-assets editor exposes the
same typed controls and routes the recovered words through its checked, undoable aggregate
transaction, so the mapping is usable for profile-declared expanded tables rather than only
portable level files.

A reciprocal Lunar Magic 3.63 export oracle now closes the installed Super GFX storage variants.
It first validates that the retained ROM owns the recovered `$6E00` expanded-settings allocation;
the ordinary level-save fixture instead has an unrelated `$8000` RATS block at the same first-fit
address and is deliberately rejected as evidence. On the owned fixture, Rust writes ten distinct
available GFX files with word-0 bit 15 both clear and set. Lunar Magic exports the exact dormant
selectors when disabled and the same selectors when enabled, proving that disabled state does not
canonicalize an owned record. The oracle repeats both states with the authentic copier header and
with that header removed, preserves the header bytes exactly, repairs the checksum, reopens the
typed record, proves byte-exact Undo/Redo, and requires identical logical ROM output across both
physical forms. This complements the exhaustive 12-bit model boundary with live resource-valid
selectors instead of mistaking nonexistent ExGFX IDs for a storage failure.

The profile-backed project layer can now resolve an enabled Super GFX selection into all six FG/BG
and four sprite payloads in native slot order, with errors retaining the exact slot and file number.
It selects 2bpp, 3bpp, or 4bpp decoding from the native `$800`/`$C00`/`$1000` decompressed lengths,
covering the special GFX01/GFX17 path instead of incorrectly assuming every file is 4bpp. Disabled
bypasses perform no pointer reads. The renderer concatenates the decoded payloads into six
foreground/background and four sprite 128-tile VRAM slots. A framebuffer fixture proves that a
bypassed tile drives the final RGBA pixel. The live ROM assets editor now goes beyond validation:
it reads the exact native two-plane 32×32 Layer 2 word layout, retains whole-cell flip bits, loads
the profile's installed Map16 set, and uses the staged per-level palette. Layer 1 and object-backed
Layer 2 now run through the same recovered tileset-family dispatch, shared/tileset extended-object
definitions, `$1B0` horizontal page stride, and vertical cache mapping as the audited usage scanner.
The resulting bypass-aware framebuffer composites Layer 2 before Layer 1 at the recovered
mode-specific editor dimensions and reports unresolved command IDs instead of fabricating tiles.
A renderer-side ordered cache-write journal now supplies sparse object placements: it omits the
initialized blank cache, collapses repeated construction writes within one object, and retains
later same-cell object overwrites in serialized painter order.
The installed native framebuffer now carries composition per Map16 placement and implements the
routine's exact averaged RGB operation. The already-proved object-tileset-4 `$027`–`$02A` condition
selects it for object or tilemap placements; all other installed placements remain opaque. Object
placements compare their complete 15-bit foreground definition, while compressed Layer 2 removes
the two whole-cell flip bits from its local word. Thus foreground `$4027` is distinct from `$0027`,
but flipped Layer 2 word `$C02A` still selects local tile `$02A`.
The recovered Layer 2 render-property bit 6 is modeled separately: modes `$0C` and `$0D` route
tilemap placements through source-only half color, matching
`RenderTransparentLevelBackgroundMap16Tile` rather than averaging against the combined framebuffer.
A retained Lunar Magic-created level `$000` test additionally requires this path to materialize
nonblank cells without unresolved definitions. Standard sprite records now use their decoded
native placements and the existing authenticated preview dispatch. Ordinary preview definitions
are composited above the object layers from the four selected SP slots with Lunar Magic's
column-major 2×2 definition order, nine-bit tile addressing, flip bits, and CGRAM rows 8–15. The
retained level `$000` fixture must also materialize at least one sprite preview. Definitions that
select bit `$0200` resolve per subtile through the separately decoded GFX33 display page recovered
from `LoadAnimationAndPlayerGraphicsCaches`; mixed-page definitions therefore retain their exact
ordinary/animated source instead of treating the page bit as an offset into SP1–SP4.

The actual Super GFX Bypass dialog callback at `0040CA80` is now defined as
`HandleSuperGfxBypassDialog`. It exposes four animation switches, not additional graphics-file
fields. `EncodeExAnimationFeatureDisableFlags` at `00460340` proves their inverted packed mapping:
bit 7 disables palette animation, bit 6 disables the vanilla animated-tile groups, bit 5 disables
global ExAnimation, and bit 4 disables level ExAnimation. Bits 0–3 are retained independently.
`InitializeExAnimationFeatureFlagsFromRom` reads the byte from either the legacy scalar location or
the indexed 512-level table, while `WriteExAnimationFeatureFlag` upgrades the scalar form to that
table when needed. Binary MWL export stores the same feature byte in the low byte of ExAnimation
metadata word 0. Rust now models all four positive option states, round-trips every possible byte,
preserves the unrelated low nibble and upper MWL metadata bits, and exposes the options through the
typed MWL GUI and edit-script transaction.

Assembly at `00460390` and `00460440` refines the installed-ROM representation. The byte at
`table_base - 1` is a sentinel: zero selects the 512-byte indexed table, while any nonzero value is
the legacy form and makes the editor load feature byte zero for every selected level. On the first
write from that legacy form, Lunar Magic emits one zero sentinel plus a zero-filled `$200`-byte
table, stores the selected level's byte, and then forces level `$110` to `$30`. That final assignment
also wins when `$110` is the selected level. A nonzero written feature byte additionally invokes
`EnsureExpandedExAnimationRuntimeInstalled`; storing the table alone is therefore not proof that
the expanded ExAnimation runtime is active. The table address is the mapped operand installed at runtime
offset `+$46` by `InstallExpandedExAnimationRuntime` (`0045CAF0`). Rust's installed storage model
now duplicates these representation and migration rules, reports the runtime-install requirement
separately for raw-layout callers, and performs checked checksum-repaired undoable writes. Ghidra
caller analysis distinguishes that requirement from `InstallExAnimationFeatureControlRuntime`
(`004606B0`): `WriteExAnimationFeatureFlag` does not call or test that patch, and installed saves
already prove the required expanded runtime through their outer installation gate. A live
Lunar Magic 3.63 `-ExportLevel` oracle also reads a Rust-persisted `$5B` feature byte from the
retained installed-ROM fixture and emits the same byte in MWL metadata, covering every inverted
feature bit and the preserved low nibble.

Verified symbol coverage after this pass: **1,399 named functions out of 3,912 total; 2,513 autogenerated `FUN_...` symbols remain.**

The graphics and level-payload cleanup block from `00481700` through `00482f00` is now named and annotated. It includes standard GFX, ExAnimation GFX, ExGFX, and special-GFX PC-offset table loaders; payload-span clamping and object/compressed-stream measurement; safe unlink-and-erase helpers for Layer 1, Layer 2, palette, sprite, and ExAnimation data; and deleted-level settings reset. The cleanup routines build a five-table reference index with the current level excluded, preventing shared ROM payloads from being erased.

Verified symbol coverage after this pass: **1,414 named functions out of 3,912 total; 2,498 autogenerated `FUN_...` symbols remain.**

The level-save-adjacent workflow through `00487820` is now named and annotated. This pass identifies internal-emulator sprite serialization, bulk graphics-pointer rewriting and integrity-word updates, ROM level-access restriction, directory-wide MWL insertion and export, rendered level-image export, bitmap-driven level deletion, hexadecimal level-list parsing, usage-report generation, and migration followed by clearing of the original SMW level-data area. The deletion coordinator explicitly protects shared Layer 1, Layer 2, sprite, palette, and ExAnimation payloads before resetting settings and secondary exits.

The level-access restriction path has since been recovered and authenticated end to end against
Lunar Magic 3.63. `HandleRestrictLevelAccessCommand` at `00485050` drives the version-1.1 dialog,
uses `Super Peachy World` for an empty title, optionally publishes an IPS patch, and calls
`RestrictLevelAccessInRom` at `004849b0`. The mutation XOR-protects nine level-save pointer words,
the two 50-entry graphics-pointer planes, and two graphics integrity words with fresh key material;
installs 32-byte per-level and bulk-save decoders at headerless PC `$06F100` and `$06F1A0`; writes
low-bank `JSL` hooks at `$028605` and `$0038DE`; sets the completion, restriction, title, and
version metadata; and fills `$07F005..$07F01D` with `$FF`. One byte at `$07F01E` compensates the
additive checksum so the stored checksum remains valid, while a present copier header receives a
terminal byte of one. A fixed-key Rust transaction over the authenticated headered level-save
fixture produces the exact full-ROM Lunar Magic FNV-1a64 value `33594e98bc236465`, with no
remaining byte differences.

Port-8089 revalidation of the same path establishes that non-LoROM support cannot be implemented by
shifting this fixed layout. Command `$23A5` reaches `HandleRestrictLevelAccessCommand`. The original
first bulk-resaves every accessible level, then `InstallPerLevelSaveMetadataPatch` obtains its hook,
code, and completion locations from descriptor indexes `$E0`, `$E1`, and `$13B`; the bulk-save
family uses `$147` and `$146`, while graphics integrity starts from `$112`. On an authentic ExLoROM
process, flag `00E278FA` is set and both effective editor metadata copies at logical `$007FBF/$007FDB`
and `$407FBF/$407FDB` receive the restriction marker/version. The active mapped descriptor values
are not a uniform `+$400000` transform. The workflow also rewrites all 52 standard graphics
pointers, optional ExAnimation slots `$80..$FF`, and optional ExGFX slots `$100..$FFF`, then updates
both authenticated graphics-integrity records. Rust therefore continues to reject ExLoROM and SA-1
before mutation rather than applying its exact LoROM-only shortcut. The regression
`exlorom_variant_is_rejected_atomically_instead_of_using_lorom_offsets` converts an authenticated
installed source, preserves its complete 8 MiB snapshot and existing history, and proves the
mapper mismatch is failure-atomic. Full mapper parity requires modeling the original bulk-resave
and descriptor-driven graphics families, plus authentic before/after fixtures for each mapper.

Verified symbol coverage after this pass: **1,433 named functions out of 3,912 total; 2,479 autogenerated `FUN_...` symbols remain.**

The level-editor mouse interaction block from `00487ce0` through `004880a0` is now named and annotated. It captures and confines the cursor during editing gestures, reports selection dimensions and movement or resize deltas, and initializes and updates tile-aligned move and resize drags with handle-specific axis constraints.

Verified symbol coverage after this pass: **1,441 named functions out of 3,912 total; 2,471 autogenerated `FUN_...` symbols remain.**

The companion editor-selection block through `00489880` is now named and annotated. It covers sprite and design-screen dragging, incremental selection rectangle repainting, drag finalization with undo capture, object and sprite ordering commands, object resizing, Direct Map16 property changes and reference remapping, and creation of a Direct Map16 rectangle from decoded clipboard dimensions.

Verified symbol coverage after this pass: **1,458 named functions out of 3,912 total; 2,454 autogenerated `FUN_...` symbols remain.**

Clipboard and level-navigation helpers through `00489cd0` are now named and annotated. These decode clipboard Map16 rectangles into objects, refresh screen-exit overlays, maintain a bounded level-and-viewport navigation history, navigate backward and forward with modified-level handling, open the screen-exit editor for a clicked cell, and follow primary or secondary exits while rejecting overworld-only destinations.

Verified symbol coverage after this pass: **1,466 named functions out of 3,912 total; 2,446 autogenerated `FUN_...` symbols remain.**

Editor input dispatch helpers through `0048ae80` are now named and annotated. This block implements horizontal swipe navigation with modifier-key behavior, manual sprite and object editing by selection or clicked cell, resize-handle hit testing and cursor selection, Alt-dragging of the design screen, Map16 tile picking, and the object-mode and sprite-mode left- and right-button handlers that coordinate selection, insertion, duplication, editing, and drag startup.

Verified symbol coverage after this pass: **1,478 named functions out of 3,912 total; 2,434 autogenerated `FUN_...` symbols remain.**

Viewport scrolling and coordinate-scaling helpers through `0048b150` are now named and annotated. They implement clamped horizontal and vertical scroll deltas plus signed and unsigned point and rectangle conversions between base editor coordinates and the active scaled display coordinate system.

Verified symbol coverage after this pass: **1,485 named functions out of 3,912 total; 2,427 autogenerated `FUN_...` symbols remain.**

The zoom-menu and exit-validation block through `0048b990` is now named and annotated. It constructs and synchronizes the editor zoom menus and toolbar state, clamps and commits 100-5000 percent zoom values, resolves Map16 acts-like chains with cycle detection, derives exit-enabled tiles from base and acts-like properties, and reports screens whose exit-enabled objects lead to invalid destinations. Explicit prototypes were applied to `UpdateEditorZoomMenuRadioCheck`, `InitializeEditorZoomState`, `GetEditorZoomState`, and `ReportInvalidExitObjectDestinations`.

Verified symbol coverage after this pass: **1,499 named functions out of 3,912 total; 2,413 autogenerated `FUN_...` symbols remain.**

Save-time diagnostic and Map16-description helpers through `0048bf90` are now named and annotated. They report sprite-count overflow, out-of-bounds object tile placement, and vertical-fireball buoyancy hazards with optional save confirmation. `FormatMap16TileBehaviorDescription` resolves acts-like chains and combines object encoding qualifiers with localized behavior labels for water, lava, power-ups, exits, Direct Map16, and other canonical tile behaviors.

Verified symbol coverage after this pass: **1,503 named functions out of 3,912 total; 2,409 autogenerated `FUN_...` symbols remain.**

External object metadata and editor hover-help functions through `0048e140` are now named and annotated. The Group B lookup helpers select metadata variants from object command bytes with default fallback. The tooltip formatters decode standard, extended, custom, Direct Map16, and tileset-specific objects plus sprites and entrance markers, including extension bytes and configuration-dependent descriptions. The hover pipeline resolves sprites first, then searches enabled object layers, accounts for zoom, and applies metadata-provided tooltip geometry.

Verified symbol coverage after this pass: **1,513 named functions out of 3,912 total; 2,399 autogenerated `FUN_...` symbols remain.**

The object-mode and sprite-mode mouse-move dispatchers and the SuperExGFX configuration coordinator through `0048e900` are now named and annotated. The motion handlers combine hover help, coordinate and selection status, drag-state dispatch, modified-state tracking, and cursor selection. The configuration coordinator installs expanded prerequisites as needed, loads standard and extended GFX pointer tables, opens the requested dialog family, preserves the legacy bypass list where appropriate, and refreshes graphics resources.

Verified symbol coverage after this pass: **1,516 named functions out of 3,912 total; 2,396 autogenerated `FUN_...` symbols remain.**

Legacy animated-graphics configuration and the title-screen recording/playback subsystem through `0048ffd0` are now named and annotated. This includes temporary recording ASM installation and removal, recording and playback payload relocation, gzip-aware Snes9x savestate validation and tagged RAM/PPU extraction, ZSNES SRAM extraction, generation of a minimal ZSNES V143 savestate containing movement data, ROM playback-data loading, and the end-to-end savestate import workflow.

The retained title-recorder interaction further resolves main command `$232D` as the Overworld
Editor opener and Overworld commands `$1F40/$1F44/$1F45/$1F46/$1F47` as save, insert playback,
export playback, install recorder, and uninstall recorder. The `$1F46` transaction publishes a
178-byte RATS runtime, redirects logical hooks `$0021DA` and `$02D79B` through low-bank LoROM
pointers, and preserves the stored checksum with the bounded `$07EFA3..$07F08D` additive run.
`$1F47` erases only the authenticated owner, restores both exact hooks and the compensation run,
and reproduces the complete source ROM byte-for-byte.

Verified symbol coverage after this pass: **1,532 named functions out of 3,912 total; 2,380 autogenerated `FUN_...` symbols remain.**

Title-screen recording export and general editor command infrastructure through `00491420` are now named and annotated. This pass identifies ZSNES savestate export, a mapper-dependent ROM addressing compatibility patch invoked during level saves, RATS scan/repair statistics and dialog population, ExAnimation window activation, selection clearing during editor-mode changes, cursor-monitor-aware editor positioning, DPI-derived window sizing and default zoom, and the main normalized keyboard-shortcut dispatcher.

Verified symbol coverage after this pass: **1,541 named functions out of 3,912 total; 2,371 autogenerated `FUN_...` symbols remain.**

File-drop routing and main-window identity helpers through `004926b0` are now named and annotated. Dropped ROM or project files, MWL levels, Map16 data, palettes, and supported savestates are routed through their respective save prompts and loaders. The adjacent helpers construct the Lunar Magic or Easter-egg title, apply main-window icon presets, and compute the five-date seasonal Easter-egg code.

Verified symbol coverage after this pass: **1,549 named functions out of 3,912 total; 2,363 autogenerated `FUN_...` symbols remain.**

The main editor UI dispatch boundary is now identified. Map16 change propagation, numeric-keypad dispatch, Direct3D renderer synchronization, IPS export/import workflows and their lower-level patch helpers are named. `HandleLevelEditorCommand` at `00492b80` is the central WM_COMMAND switch for editor menus and toolbars. `LevelEditorWindowProc` at `00498fa0` handles creation, paint, input, scrolling, drag-and-drop, sizing, focus, DPI, commands, and shutdown; it now has the explicit prototype `LRESULT __stdcall LevelEditorWindowProc(HWND, UINT, WPARAM, LPARAM)`.

Verified symbol coverage after this pass: **1,558 named functions out of 3,912 total; 2,354 autogenerated `FUN_...` symbols remain.**

The first application-lifecycle and settings-persistence block through `0049b340` is now named and annotated. It captures main and secondary editor window geometry, loads and saves the ten-entry recent-file list, persists two external-tool profiles, maintains variant-specific original-ROM paths and current ROM path components, and round-trips packed `Manifest` and `DPIAwareness` registry option bits while preserving unknown fields.

Verified symbol coverage after this pass: **1,575 named functions out of 3,912 total; 2,337 autogenerated `FUN_...` symbols remain.**

`SynchronizeApplicationSettingsRegistry` at `0049b400` is now identified, annotated, and prototyped as `void SynchronizeApplicationSettingsRegistry(char loadMode)`. It is the consolidated bidirectional serializer for the three packed option DWORDs, seven editor zoom and last-DPI profiles, animation rates, undo limits, restore settings, the 256-byte palette map, main and secondary window geometry, manifest flags, and DPI-awareness flags. Nonzero mode loads and expands settings; zero mode packs and saves them while preserving unknown registry bits.

Verified symbol coverage after this pass: **1,576 named functions out of 3,912 total; 2,336 autogenerated `FUN_...` symbols remain.**

The application startup and main-frame boundary through `0049ccf0` is now named and annotated. It registers the `.mwl` file association, canonicalizes startup paths and component offsets, parses quoted ROM/project/level arguments, queues startup opening, validates the executable with a custom rolling integrity calculation, and caches total physical memory. `MainFrameWindowProc` creates the menus, MDI client, and level-editor child and handles commands, DPI, toolbar/rebar/status layout, drops, close prompts, and nonclient compatibility; it now has the prototype `LRESULT __stdcall MainFrameWindowProc(HWND, UINT, WPARAM, LPARAM)`.

Verified symbol coverage after this pass: **1,583 named functions out of 3,912 total; 2,329 autogenerated `FUN_...` symbols remain.**

ROM stream acquisition, shared-palette saving, and the vanilla animated-graphics update path through `0049dd90` are now named and annotated. The stream helpers distinguish simple opening from timestamp-aware external-change auditing. The graphics helpers expand packed nibbles, render tileset- and option-dependent animation groups, advance configured tick batches and dynamic colors, swap adjacent tile-bank blocks, and load the four graphics-file assignments for the active FG/BG setting.

Verified symbol coverage after this pass: **1,592 named functions out of 3,912 total; 2,320 autogenerated `FUN_...` symbols remain.**

The core Map16 and level graphics working-set rebuild block through `0049e990` is now named and annotated. It loads Layer 2 and sprite graphics-file assignments, derives Layer 3 special graphics state, decodes base Map16 pages using a presence mask and alternating streams, performs tileset page substitution, initializes empty Map16 and acts-like tables, loads extended Map16 data, converts SNES tile attribute layout into editor layout, initializes animation buffers, and coordinates dependent editor refreshes.

Verified symbol coverage after this pass: **1,601 named functions out of 3,912 total; 2,311 autogenerated `FUN_...` symbols remain.**

The application bootstrap and bundled zlib/DEFLATE region through `004a61b0` is now substantially labeled and annotated. This pass identified the command-line operation dispatcher, WinMain-style application lifecycle, DPI and activation-context setup, dialog CBT subclass hook, runtime teardown, Adler-32 calculation, incremental inflate state machine, dynamic-output inflate wrapper, canonical Huffman construction, fixed/dynamic block emission, fast and lazy LZ77 encoder paths, encoder stream state, and output-buffer callbacks.

Verified symbol coverage after this pass: **1,640 named functions out of 3,912 total; 2,272 autogenerated `FUN_...` symbols remain.**

Analysis now continues through `004ac230`. The remaining codec wrappers were identified, including heap-output compression and RGB24-to-PNG encoding. The ROM backing-stream, memory-image, expansion, ZSNES compatibility-lock, ExLoROM null-bank-lock, STAR/RATS allocation, mapper-aware free-space scanning, and fragmentation-repair routines are named and annotated. The following LZ2/LZ3 encoder helpers and inserted-ASM relocation engine have also been separated into semantic functions covering match indexes, literal commands, transformed matches, metadata validation, relocation requests, patch tables, and ROM/SNES address rewriting.

Recovered data types added in Ghidra: `RatsAllocationHeader`, `CodecGrowableOutput`, and `LzMatchIndexNode`. Explicit prototypes were applied to `AppendCodecOutputToBuffer`, `WriteRatsAllocationHeader`, and `ShowRelocationEngineError`.

Verified symbol coverage after this pass: **1,687 named functions out of 3,912 total; 2,225 autogenerated `FUN_...` symbols remain.**

The relocation metadata parser and restore-system implementation through `004b1220` are now named and annotated. Recovered behavior includes LMRE/LOC1 typed relocation sections, auxiliary ROM file I/O, incremental CRC-32, original-ROM discovery by expected checksum, `.lrp` archive creation and validation, compressed payload staging, restore record and linked-directory finalization, `.extmod` and other sidecar tracking, full and delta restore-point creation, ROM reconstruction, reversion records, and automatic full-restore policy.

New corrected data types: `LmRelocationSectionHeader` and the padded 256-byte `RestoreDirectoryRecord`. A preliminary sparse definition was removed after confirming that this MCP endpoint packs fields sequentially; the replacement explicitly models all intervening padding and verifies at 256 bytes.

Verified symbol coverage after this pass: **1,738 named functions out of 3,912 total; 2,174 autogenerated `FUN_...` symbols remain.**

The restore-point list/dialog UI and ExAnimation ROM persistence region through `004b4200` are now labeled. This includes archive list traversal and formatting, save-point selection, restore options and tooltips, ExAnimation engine installation/upgrade, seven level-slot deserialization and serialization, global record storage, packed slot-option flags, allocation cleanup, and coordinated commit behavior.

An isolated Lunar Magic 3.63 command `$23B9` observation now covers that restore dialog itself.
`HandleRestorePointDialog` (`$004B1990`) initially leaves the single owner-drawn record unselected,
defaults auxiliary restoration on, and refuses submission until a row is selected. Submission shows
the destructive `WARNING!` prompt. No preserves the selected dialog and every byte; Yes closes the
open ROM, restores the ROM and all thirteen files, and appends the successful reversion record.

The 256-byte `RestoreDirectoryRecord` type is now applied to its live global buffer at `00931b48`. The restore archive header storage and all four seven-byte ExAnimation option arrays are also typed, renamed, and plate-commented in the listing.

The `.lrp` reconstruction boundary is now independently authenticated in Rust. The archive begins
with a `$130`-byte prefix containing the `LR` version header, producer string, and 64-bit
first/latest record links. Each linked record has a `$100`-byte header with reciprocal links,
payload/description bounds, `DIRL` at `+$3C`, record ID, packed date/time, ROM size and ROM hash.
When directory-version bit `$4000` is set, the payload is a raw DEFLATE stream. Its decoded byte-sum
XOR `$FADEC0DE` must match the stored checksum. Commands use control bits for fill versus raw copy,
24- versus 32-bit destination, and a one- through four-byte length; `$FF` terminates the stream.
Against the authentic Lunar Magic 3.63 archive, record 2 decoded to 525 commands and reconstructed
the 2,097,664-byte target ROM exactly from `smwOrig.smc`. The Rust reader rejects cycles, broken
back-links, invalid record/payload ranges, checksum failure, malformed controls, missing or early
terminators, overflow, output beyond the 16-MiB ROM bound, and a logical-ROM CRC-32 mismatch before
publication. The authenticated original and restored logical hashes are `$B19ED489` and
`$ED863127`. The native closed-project workflow mirrors the original `HandleRestorePointDialog`
guard, displays ID/date/time/type/description columns, defaults to the newest point, reconstructs
off-thread, and replaces only an explicitly selected existing regular ROM through staged atomic
publication. Ghidra confirms thirteen associated-file slots at record offset `$80`, ordered
`msc`, `dsc`, `ssc`, `m16`, `s16`, `mwt`, `mw2`, `sscov`, `s16ov`, `lmtbl`, `mw0t`, `mw0`, and
`osc`. A nonzero record-relative offset replaces the inherited slot value; the owning record's
`$4000` flag selects raw DEFLATE, and formats before 3.21 encode the end offset for stored sizes
above `$10000`. The archive header retains the corresponding thirteen 64-bit timestamps at `$40`.
The Rust workflow implements these rules and can publish a restored ROM plus existing or absent
sidecars as one rollback-safe group. The authentic local two-record LM 3.63 archive has all 26
sidecar entries empty, so synthetic structural fixtures cover inheritance, inflation, and legacy
offset behavior pending capture of an authentic nonempty-sidecar archive.

Creation-side validation corrected two preliminary field names using the authentic file itself.
Archive offset `$08` is the latest assigned record ID (the original increments it when opening for
the next write), while record offset `$18` is the complete record extent rather than an inflated
payload size. The authentic values prove both boundaries exactly: `$130 + $124 = $254`, the second
record's address, and `$254 + $1FCC0 = $1FF14`, the archive file size. The stored-record checksum
covers header bytes `$30..$FF`, the terminated description, stored payload, and changed stored
sidecars, XORing `$FADEC0DE` for compressed records or `$C001C0DE` otherwise. Rust now validates
those invariants and creates native one-record full archives with bounded ROM deltas, optional raw
DEFLATE, all thirteen sidecars, timestamps, producer metadata, both checksums, record extent, and
logical-ROM CRC-32. Compressed and uncompressed archives decode and reconstruct themselves exactly;
append, delta/reversion policy, and graphical creation controls remain.

Linked creation now covers both ordinary deltas and later full checkpoints. Appending patches the
old last record's forward pointer, stores its absolute address in the new record's backward link,
advances the latest ID and sequence, replaces the archive date/timestamp/hash and complete thirteen
timestamp snapshot, and reseals the new stored-record checksum after its type and identity fields
change. Delta append requires the caller's base ROM size and logical CRC-32 to equal the archive
tip. Full checkpoints retain format bit 0 and encode against the original ROM; deltas clear the
low two type bits and encode against the prior state. Restore and sidecar resolution now begin at
the latest full checkpoint at or before the selected ID, matching the backward scan in
`RestoreRomToSavePoint`, instead of incorrectly replaying every historical record. Synthetic
three-record archives prove reciprocal links, IDs/sequences, timestamp propagation, delta
inheritance, full-checkpoint reset, and exact restoration.

`RecordRestorePointReversion` confirms that reversion records set format bit 2, store their selected
record as a separate 64-bit link at record `$10`, contain no ROM command payload, preserve the
selected record sequence, and retain the restored ROM size/hash. Reconstruction walks backward
through ordinary `$08` previous links but substitutes the `$10` target link for reversion nodes,
skipping those nodes when applying commands and resolving sidecars. A second consecutive reversion
truncates and replaces the previous marker at the same archive offset, reusing its ID rather than
growing the file. Rust implements that graph, successful reversion creation, replacement behavior,
and later deltas based on a reverted state. Synthetic full/delta/reversion/delta chains restore
exactly.

The failed branch of `RecordRestorePointReversion` uses the same target graph but appends
`" (failed?)"` to the description and zeros both the archive ROM timestamp and resulting ROM hash.
Rust now emits and validates that form without requiring a partially restored ROM to match the
target. `PrepareAutomaticRestorePoint` independently confirms that a timestamp or logical-CRC
continuity break forces a full checkpoint before interval evaluation; otherwise the zero-based
sequence since the last full point is compared with the configured interval. The recovered
automatic-full descriptions distinguish continuity/external-program, interval, daily, and original
ROM reasons. The pure Rust policy returns continuity-break first, then interval, then daily
rollover, otherwise delta; focused tests cover every branch and priority collision. The native File
menu now creates a one-record full archive from the open project's in-memory snapshot, captures all
thirteen associated-file slots in native order, records Windows-format file timestamps, validates
the newly encoded archive before publication, and refuses to overwrite an existing destination.
A native round-trip fixture reconstructs both a changed ROM and nonempty `.msc`/`.s16ov` sidecars.

The native File menu now also exposes manual delta, manual full, and automatic append operations.
Before a delta append it reconstructs the current archive tip and uses that exact state as the
delta base; before an automatic append it compares the saved ROM timestamp and logical hash with
the archive header. A fresh decompile of `ArchiveChangedAssociatedFiles` at `004af5c0` confirms that
LM selects changed sidecars by comparing each captured 64-bit file timestamp with the archive
header. Missing files clear the current timestamp but do not emit an empty replacement payload.
The native append path follows those rules and atomically replaces only the selected archive after
the appended result decodes successfully.

The native restore dialog now records reversions as part of the restore transaction. A successful
restore constructs the selected ROM and sidecars, appends a reversion node targeting that record,
and atomically publishes the archive, ROM, and sidecars together. If reconstruction or publication
fails, it attempts Lunar Magic's failed-reversion form instead: the description gains
`" (failed?)"` and the resulting ROM timestamp/hash are zero. Focused fixtures verify both the
successful graph link and failed marker fields.

The automatic-policy dialog remembers its interval and daily selections for later automatic points
and stores them through the native frontend's versioned application storage for subsequent
launches. Strict decoding rejects missing, extra, zero-interval, and non-Boolean fields without
silently changing restore behavior.

Verified symbol coverage after this pass: **1,762 named functions out of 3,912 total; 2,150 autogenerated `FUN_...` symbols remain.**

The post-ExAnimation per-level metadata and expanded Map16 region through `004b7fc0` is now substantially labeled. This pass separates four-table and packed 5+5+2-byte level metadata patches, dual compressed interleaved table storage, expanded Map16 acts-like data, legacy/current Map16 remap record formats, allocation cleanup, format dispatch, tile-space resizing, reference adjustment, and legacy Map16 runtime-hook normalization.

Recovered `Map16RemapRecordNode` as a 16-byte doubly linked editor node containing source/destination tiles, flags, group index, and editor state. The type is applied to the live list-head global at `00e27ea0`, now named `g_pMap16RemapRecordHead` with a plate comment.

Verified symbol coverage after this pass: **1,801 named functions out of 3,912 total; 2,111 autogenerated `FUN_...` symbols remain.**

The overworld event and level-name region through `004bc720` is now named and annotated. Recovered functionality includes legacy Map16 low-byte acts-like storage, event reveal dependency chains, main/special reveal tables, event tilemap compression streams, event-number mapping, the associated runtime patch fragments, overworld level metadata orchestration, broad overworld feature-hook detection/commit, and original/expanded overworld level-name decoding and storage.

The native level-name persistence boundary is now reproduced in Rust. Vanilla names use 93
two-byte codes, a 59-word dictionary partitioned 31/15/13, and a 460-byte high-bit-terminated text
blob. Expanded names are positional 19-byte records (up to 256) in a RATS allocation addressed by
the runtime operand at `+0x37`; both hooks target the fixed 96-byte runtime. Slot `0x00..0x24` maps
to levels `0x000..0x024`, then slot `0x25` resumes at level `0x101`. Installation, growth,
checksum repair, semantic reopen, and exact undo are covered by pristine-ROM and built-CLI tests.

The adjacent player-start boundary is also recovered. Descriptor `+0x58C` is headered physical
offset `$0020F0`, hence logical `$001EF0`, and supplies one exact 22-byte runtime-options block.
Bytes 0/1 are Mario/Luigi submaps; words at `+6/+8` and `+A/+C` are their pixel X/Y coordinates;
words at `+E/+10` and `+12/+14` redundantly store those coordinates shifted right four. Bytes
`+2..+5` belong to adjacent runtime options and are retained losslessly. Descriptor `+0x590`
identifies logical `$02B15D`; Lunar Magic writes `EA EA EA` there when either player differs from
the vanilla submap-1 `$68,$78` start. Rust loading, guarded writes, checksum repair, and exact undo
are covered against the pristine ROM. The 34-byte `LMOWST1` interchange file has a 12-byte
versioned header and the exact 22-byte runtime block. Built CLI and application workflows export,
import, validate the custom-start patch precondition, repair the checksum, semantically reopen both
players, and retain bytes `+2..+5` without assigning them speculative meanings.

Added the 19-byte `OverworldLevelName` type and applied a 93-element array at `00b08168`. The 256-byte event-number map and both 255-entry event reveal word tables are typed, renamed, and plate-commented at their live global addresses.

Verified symbol coverage after this pass: **2,270 named functions out of 3,912 total; 1,642 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after the bitmap-import orchestration pass: **2,286 named functions out of 3,912 total; 1,626 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after the bitmap-import UI pass: **2,302 named functions out of 3,912 total; 1,610 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after the import-controller and tile-remapping/clipboard pass: **2,324 named functions out of 3,912 total; 1,588 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after native, legacy, and bitmap clipboard-paste recovery: **2,330 named functions out of 3,912 total; 1,582 autogenerated `FUN_...` symbols remain.**

`ImportClipboardBitmapAsMap16` (`004f5b00`) and `PasteTilesFromClipboard` (`004f6050`)
were also exercised end to end against a disposable Lunar Magic 3.63 Wine process. Bitmap paste
dispatch requires either editor mode byte `00e277cc == 2` or alternate guard `00e27de5 != 0`;
setting the former only while posting command `$2276` opens the conversion dialog without changing
the persistent editor mode. `tools/lunar-magic-bitmap-import-audit.sh` automates that sequence,
authenticates the loaded-state byte at `00e2782a` and current level dword at `005e7738`, and reloads
that exact level through Lunar Magic's own level-number workflow before observing buffers. This is
required because Wine can restore the modeless editor before ROM loading, leaving a valid HWND
backed by stale palette and graphics state. The script opens the modeless Map16 parent through the
authenticated main-editor command `$232f` when its HWND is absent, and repairs a stale
persisted-open byte only in that impossible null-HWND state. Ghidra
`HandleLevelEditorCommand` at `00492fdf` proves the `$232f` route through
`RestoreOpenAuxiliaryEditorWindows` and `ShowMap16EditorDialog`; the audit retains a bounded five-second
creation gate and still requires a disposable process with a ROM loaded. It captures the live
256-entry RGB32 palette at `00758dd8` and 64-KiB planar graphics cache at
`0086b7e8` before and after acceptance, records the dialog controls, and restores the guard on every
exit path. Popularity captures select and record reduction mode, priority 1..4, and maximum colors
1..128 rather than relying on dialog defaults. `HandleBitmapImportOptionsDialog` maps control `$6E`
to `DAT_005e55ce`; `SelectOptimalColorsFromRgb555Histogram` at `004ebf30` gates its complete
nearest-color distance weighting on that byte. The Rust Popularity reducer therefore exposes the
same independent “Give higher priority to unique colors” switch, and the audit captures both its
checked and unchecked state. Repeated level-105 captures proved palette entry
`$64` is live animation state: it can change while graphics output remains byte-identical and must
not be misclassified as a reduction-priority result. A four-color 16×16 oracle converted to exact
SNES words `$77b4,$7fb6,$7ff9,$7ffe` and
changed 53 graphics bytes beginning with tile `$200`. This proves that even the no-reduction path
applies Lunar Magic's channel rule: truncate to five bits, then round upward when source bit 2 is
set unless already at 248. The Rust low-color and Popularity histogram paths now share that rule.

The same color-options handler maps “Maintain detail” control `$66` to `DAT_00e27b0a` (unchecked
by default). `AssignImportedGraphicsToPaletteRows` at `004ed7a0` always performs the exact-fit
color-set pass, then enters the recovered weighted partial-set extension only when that byte is
zero. `AggregatePaletteColorSetWeights` (`004ecc80`) folds linked subset weights into their
supersets, while `ExtendPaletteWithWeightedColorSets` (`004ed000`) repeatedly chooses the uncovered
set with greatest existing-row overlap and aggregate weight, inserts its strongest missing colors
by direct pixel weight up to remaining capacity, and marks covered subsets. Rust now has that default second pass and an
independent Maintain Detail switch that skips it. The same byte has an earlier role inside
`ProcessBitmapGraphicsImport` (`004ef770`): exact source-color matches claim their reduced-palette
indexes first, then one globally nearest unused source color is claimed for every remaining palette
color before ordinary nearest-color mapping. The native candidate array contains a leading zero
sentinel in addition to the requested opaque colors. A Maintain Detail breakpoint at `$004F0269`
proved the sentinel participates in the distinct-source pass and can claim the nearest unused
bitmap color as transparent. Rust reproduces that complete assignment. The Wine harness captures
both control states; the retained 16-color high-color fixture now has byte-identical palette and
graphics workspaces in both modes, while focused tests prove the sentinel claim, opaque
distinct-source assignment, and later partial-set allocation branch.

Verified symbol coverage after LM16 container, undo-history, and Map16 renderer recovery: **2,362 named functions out of 3,912 total; 1,550 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after Map16 presentation and custom-control subclass recovery: **2,372 named functions out of 3,912 total; 1,540 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after Map16 tooltip, zoom, selection movement, and property-edit recovery: **2,414 named functions out of 3,912 total; 1,498 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after Acts Like editing and complete 8x8-subtile selection/clipboard recovery: **2,446 named functions out of 3,912 total; 1,466 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after native Map8 clipboard entry points and Map16 render-window dispatch recovery: **2,460 named functions out of 3,912 total; 1,452 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after Map16 parent-dialog and graphics tile-editor rendering recovery: **2,486 named functions out of 3,912 total; 1,426 autogenerated `FUN_...` symbols remain.**

Verified symbol coverage after completing the graphics-editor input/window lifecycle and beginning the overworld graphics tile browser: **2,518 named functions out of 3,912 total; 1,394 autogenerated `FUN_...` symbols remain.** The newly named range through `005089a0` includes single-tile clipboard operations, pixel hit testing and cursor confinement, command/keyboard/character dispatch, DPI-scaled control and font layout, the complete 8x8 editor window procedure, its compiler-split cleanup tails, monitor-aware window creation, and the overworld tile browser renderer, palette/flip property synchronization, keyboard navigation, and window procedure.

Verified symbol coverage after the first full overworld 8x8 tile-viewer canvas-infrastructure and zoom pass: **2,556 named functions out of 3,912 total; 1,356 autogenerated `FUN_...` symbols remain.** The named range through `0050ae00` documents hexadecimal dialog parsing, status and scroll-bar state, DPI/RTL-aware button icon strips, canvas and scratch-buffer ownership, gradient or palette background filling, clipped text blending, selection-frame drawing, active palette modes, page scrolling, scaled-coordinate conversion, zoom menus and status, 100-5000 percent zoom state, and DPI-aware canvas/parent sizing. This subsystem identification is proven by its registered `Window8x8viewOvx` class and native clipboard format string `Lunar Magic 8x8ov Tiles`.

Verified symbol coverage after overworld 8x8 rendering, selection, and bulk-remapping recovery: **2,586 named functions out of 3,912 total; 1,326 autogenerated `FUN_...` symbols remain.** The range through `0050dd70` identifies dialog-command and keyboard dispatch, the page rasterizer and composited canvas presentation, static and animated selection frames, page-section labels, rectangular selection-mask construction, bounded drag movement, selection status reporting, the textual bulk tile/palette remapping grammar and translation-table application, and the native 0xA0-byte overworld 8x8 clipboard allocation/layout builder.

Verified symbol coverage after native overworld 8x8 clipboard and tile-viewer window recovery: **2,601 named functions out of 3,912 total; 1,311 autogenerated `FUN_...` symbols remain.** The range through `0050f950` documents native and text clipboard publication, validated index-array extraction, programmatic single-tile selection, selection clearing and inset bounds, hover/animation status, edge auto-scroll, 256x256 back-buffer creation, the complete canvas window procedure, and creation of the `8x8 Tile Viewer` child window.

Verified symbol coverage after the overworld 8x8 parent-dialog and general graphics 8x8 viewer transition: **2,626 named functions out of 3,912 total; 1,286 autogenerated `FUN_...` symbols remain.** The range through `00513e60` adds custom push/toggle/edit subclasses, disabled-control tooltip relaying, compatibility popup positioning, the complete overworld 8x8 parent dialog and modeless show entry point, then identifies the following non-overworld graphics 8x8 viewer by its separate global state, decoded graphics source, and page labels for standard GFX, animation, player, extended-animation, Layer 3, and sprite-extra regions. Its resource ownership, status bar, canvas background/text blending, section labels, page rasterizer, and canvas compositor are now named.

Verified symbol coverage after general graphics 8x8 selection and clipboard recovery: **2,655 named functions out of 3,912 total; 1,257 autogenerated `FUN_...` symbols remain.** The range through `00515a70` documents canvas presentation, 0x4000-cell rectangular selection masks, bounded drag movement, native `Lunar Magic 8x8 Tiles` clipboard serialization, text-index fallback publication, validated clipboard extraction, programmatic tile selection with palette and flip state, hover and ExAnimation navigation, scroll initialization, back-buffer creation, and the complete canvas window procedure. This format is intentionally distinct from the overworld-specific `Lunar Magic 8x8ov Tiles` format despite their shared 0xA0-byte header layout.

Verified symbol coverage after completing the general graphics 8x8 selector and beginning level-layer canvas recovery: **2,674 named functions out of 3,912 total; 1,238 autogenerated `FUN_...` symbols remain.** The range through `00517280` completes the standalone `8x8 Tile Selector` child, tool-window procedure, monitor positioning, scale conversion, and DPI sizing. The following subsystem is identified as the 512x512 level-layer canvas from its current-level mode dispatch, boss and null-map diagnostics, level object and four-subtile map data, decoded graphics and palette rendering, selection/priority overlays, modification gating, and dirty-state propagation.

Verified symbol coverage after level-layer selection and incremental redraw recovery: **2,692 named functions out of 3,912 total; 1,220 autogenerated `FUN_...` symbols remain.** The range through `00518c00` documents full-canvas refresh, primary and secondary selection flags over the 32x32 map, coalesced dirty-mask rectangle redraw, cursor capture and clipping, hover highlights, selection dimension and movement status, rectangular selection construction and resizing, extracted-selection finalization, transfer-buffer ownership, and swapping selected map words and flags at a destination offset.

Verified symbol coverage after level-layer movement and clipboard-format recovery: **2,712 named functions out of 3,912 total; 1,200 autogenerated `FUN_...` symbols remain.** The range through `005199d0` documents constrained movement of extracted or transfer-buffer-backed selections, Map16 index-grid placement with active BG-bank validation, native selection copy/paste, and fallback paste from `Lunar Magic 16x16 Tiles`. Added the exact 128-byte `LunarMagicBgTilesV2Header`: size and two section offsets at `0x00`-`0x08`, selected count at `0x30`, extracted bounds at `0x34`-`0x40`, and reserved regions preserving the native layout. Native `Lunar Magic BG Tiles v2` payloads append the 0x800-byte level-map word array and 0x400-byte selection-flag array.

Verified symbol coverage after recovering the level-background editor presentation and rendering layer: **2,766 named functions out of 3,912 total; 1,146 autogenerated `FUN_...` symbols remain.** The newly named range through `0051df60` includes bulk tile remapping, navigation from layer cells to Map16 definitions, mouse and keyboard command handling, the canvas window procedure, resizable top-down 32-bit DIB allocation, optional `Lunar Magic.ffxhd` background overlay loading, scrolling, status-bar and toolbar icon management, current-memory-map edit gating, boss and null-map diagnostics, complete Map16 background rendering, grid and animated selection overlays, dirty-cell coalescing, and primary/secondary selection-mask maintenance.

The next level-background interaction block through `0051e8d0` is named and annotated. It covers transient secondary-selection clearing and hover updates, tile-transfer buffer reset, selection commit, localized selection/move/resize status messages, cursor capture, rectangle-selection and move/resize drag initialization, axis-constrained resizing, dirty-mask accumulation, and minimal redraw of changing marquee edges. Latest verified coverage is **2,780 named functions out of 3,912 total; 1,132 autogenerated symbols remain.**

Level-background editing through `00521cf0` is now named and annotated. The recovered paths include selection finalization, staged/live tile swapping, bounded movement and resize deltas, repeated-pattern resizing, connected-region scanline flood fill, fill sources from either the current background selection or Map16 editor, scrolling helpers, and clipboard v3 serialization with referenced eight-byte Map16 definitions and legacy 16x16-format fallback. Added the exact 2,064-byte `LevelBackgroundUndoSnapshot`: 1,024 16-bit Map16 tile words, background-bank and level-flag words, and doubly linked history pointers. Typed and named the history head/current pointers, retained count, and operation-coalescing token. Latest verified coverage is **2,827 named functions out of 3,912 total; 1,085 autogenerated symbols remain.**

The level-background editor is now traced through its canvas creation at `005264e0`. This range includes BG Tiles v3 index and eight-byte Map16-definition extraction, rectangular clipboard publishing, copy-from-level and operation dialogs, the 0x8000-entry textual tile-remap language, selection-edge and cursor hit testing, primary/secondary mouse handlers, keyboard shortcuts, independent coordinate scaling, 100-5000 percent zoom menus, DPI-aware viewport sizing, monitor constraints, Direct3D context lifecycle, the complete command dispatcher, and the canvas window procedure with optimized scroll-region rendering and wheel accumulation. Latest verified coverage is **2,869 named functions out of 3,912 total; 1,043 autogenerated symbols remain.**

`DecodeBackgroundRemapScriptTerm` (`005225D0`) and
`ApplyLevelBackgroundTileRemapScript` (`00522730`) now have a platform-neutral Rust model.
Unprefixed hexadecimal values occupy Lunar Magic's displayed `$8000`–`$FFFF` domain; `+` and `-`
apply saturating relative changes, `M` generates sequential destinations, and an `R` source range
addresses a rectangle in a 16-column Map16 page. The complete program composes operations through
a 32,768-entry identity table, applies the dialog's global saturating offset, supports selected or
all native background cells, normalizes stored words to 12-bit indexes, and reports the active
Map16 bank selected by the last cross-bank primary result. Parsing, linear/range/matrix/rectangle
semantics, selection bounds, duplicate targets, global offset, and bank transitions have focused
Rust tests.

The remapper is now connected to the native aggregate controller and Layer 2 panel. Scripts can
target the complete 32×32 background or the current rectangular selection and are staged as one
revision-bound, semantically reopenable ROM edit. The current pristine/direct-pointer profile
correctly starts in bank 0 and rejects a cross-bank result before mutation; installed-table bank
persistence remains gated on modeling that table's descriptor write rather than silently dropping
the bank transition.

`LoadLayer2ObjectDataForLevel` (`0046732F`) proves that the first Layer 2 MWL metadata word is the
lossless in-memory descriptor. Installed table generations read active Map16 bank bits 4–6 from
that word; pristine/direct-pointer backgrounds synthesize bank 0. Bit 1 selects compressed tilemap
storage and bit 2 selects the newer split-plane layout. Native remapping marks compressed storage,
retains the existing layout bit, records the result bank, and clears the legacy/direct-pointer bit.
The Rust MWL boundary now exposes a typed descriptor while retaining every unknown bit and
the opaque second source-address word. All 525 retained MWL files were surveyed: 499 carry
descriptor `$0000000C`, 26 carry `$00000000`, and none supplies cross-bank fixture coverage.

`ExportBinaryMwlLevelFile` (`004797D0`) selects `0x800` bytes from the live tilemap workspace
`DAT_007592F0` and passes that buffer directly to `fwrite`. Consequently the MWL payload retains
Lunar Magic's two row-major 16×32 planes exactly; no export-time conversion to a flat 32-column
visual raster occurs. A retained Level 000 comparison exposed and removed the Rust model's former
double transform, after which the installed RLE expansion and MWL payload agree word-for-word.

`LoadSpriteDataPcOffsetTable` (`004810E0`) resolves the other provenance dependency used by MWL
export. Descriptor entry 23 supplies 512 low words. Opcode `$22` at descriptor entry 50 selects
descriptor entry 51's 512 per-level bank bytes; otherwise entry 24 supplies one shared bank. For
the installed SMW-US fixture these are logical offsets `$02EC00`, `$02D8F5`, and `$077100`, and
level 000 resolves to `$109ED5`, matching the second sprite metadata word emitted by Lunar Magic.

`DetectLayer2DataTableFormat` (`004664E0`) resolves the active descriptor's entry `$3F` hook base;
the exact `LM $0103` marker at hook offset `$3C` identifies format `$103`. Without that marker,
opcodes `$5C`, `$4B`, and `$A9` at hook offset `$09` identify formats `$102`, `$101`, and `$100`
respectively. Rust now classifies all four generations, rejects unknown `LM` versions, and refuses
to treat legacy installed runtimes as pristine layouts. The complete `$100` hook is the recovered
predecessor of `$101`: it selects hard-coded data bank `$05` with `LDA #$05 / PHA / PLB`, while
`$101` replaces those three bytes with `PHK / PLB` and shifts the otherwise identical tail two
bytes earlier. `MigrateLayer2ObjectDataTable` independently proves both `$100` and `$101` take the
same descriptor-normalization branch before the common pointer conversion. Rust therefore
authenticates all 64 `$100` bytes rather than trusting the `$A9` discriminator alone. An authentic Lunar Magic 3.01 ROM selects
format `$102`; a disposable Lunar Magic 3.63 `-ImportLevel` run upgrades it to `LM $0103`, providing
the retained before/after migration oracle. The recovered `$102` loop reads the complete Layer 1
and Layer 2 three-byte pointer tables at logical `$02E000` and `$02E600`. Bank-`$FF` Layer 2
pointers materialize in bank `$0C`; their descriptor becomes `$08` below raw boundary `$FFE8FE`
and `$18` at or above it, with the special `$068000`/`$FFD900` pair redirected to `$FFDE54`.
Non-sentinel pointers resolve their level-mode byte: object-backed modes receive descriptor zero,
while compressed-tilemap modes retain only mask `$F6`. Rust's transactional migration reproduces
all 1,536 pointer bytes, all 512 descriptors, and the exact 64-byte `$103` hook from the authentic
3.01→3.63 oracle. In the retained installed SMW-US ROM,
the headerless hook is `$077510`, the marker is `$07754C`, and descriptor entry `$3E` points to the
512-byte per-level descriptor table at `$077310`. `ExpandLegacyLayer2TilemapLayout` (`004670D0`)
uses bits 4–6 as the legacy expansion high byte and then normalizes the descriptor with
`(raw & $0A) | $04`. `SaveLevelToRom` (`00483240`, descriptor write at `00483B90`–`00483BB8`)
writes the selected level's byte through descriptor entry `$3E`. The Rust profile, aggregate
controller, and GUI now carry that byte losslessly and publish descriptor, relocated Layer 2
payload, checksum, and undo history atomically.

The modeless background-editor parent and the following overworld Layer 2 event-tile selector are now named through `005295a0`. The parent block covers reusable momentary and toggle bitmap-button subclasses, tracking-tooltip positioning, disabled-control tooltip hit forwarding, complete dialog initialization and teardown, DPI refresh, and window show/destroy entry points. The event selector is identified by its `Window16x16viewEventov` and `WindowEventov` classes and `Layer 2 Event Tile Selector` title. Its recovered implementation includes a 192x192 top-down DIB, 24x24 visible tile grid, dual scroll axes, 28 pages, current and hover selection, seven preview frames, status messages, independent scaling, keyboard and wheel handling, DPI size negotiation, child and outer window procedures, and creation entry points. Latest verified coverage is **2,904 named functions out of 3,912 total; 1,008 autogenerated symbols remain.**

The level-object template browser is now named through its window procedure at `0052ac40`. It loads localized metadata and multiple embedded encoded-template resource banks, switches among standard, variant, alternate, and direct Map16 categories, performs case-insensitive template-description search with continuation semantics, decodes selected object-template streams, constructs direct Map16 rectangle commands, renders a 16x16 Map16 preview into a 256x256 top-down DIB, overlays search diagnostics, and provides occupied-cell tooltips and rectangle dragging. The control-notification dispatcher, category and list population, preview lifecycle, selection synchronization with the main level editor, and complete custom window procedure are annotated. Latest verified coverage is **2,930 named functions out of 3,912 total; 982 autogenerated symbols remain.**

The following custom level-object library is now named through its compatibility-status builder at `0052d700`. This editor persists encoded object streams in a ROM-adjacent `.mw0` sidecar and newline-delimited descriptions in `.mw0t`, with optional UTF-8 BOM handling. Recovered operations include appending the current object selection, deleting or reordering paired data and description records, saving or deleting empty sidecars, Unicode case-folded search, hexadecimal tile/page selection, list-box keyboard routing, `.ff7` six-button toolbar loading, preview/status rendering, external-object tileset and foreground-GFX compatibility reporting, and editor model reload. Typed and named both 32,768-byte sidecar buffers, their sizes, and the cached Unicode search-sort-key pointer and length. Latest verified coverage is **2,968 named functions out of 3,912 total; 944 autogenerated symbols remain.**

The custom-object editor is now named through its DPI-aware resize handler at `00531510`. The additional model layer decodes selected templates, generates as many as 1,024 rendered 32x32 icons, filters encoded records for the current foreground-GFX and tileset compatibility, merges the standard category groups, and converts rectangular direct-Map16 selections into placeable custom objects. The UI layer now distinguishes canvas and list hover tooltips, direct-Map16 drag creation, six zoom presets, logical/display coordinate conversion, preview sizing, saved control rectangles, monitor-aware dialog sizing, DPI scaling, and resize anchoring. Latest verified coverage is **3,027 named functions out of 3,912 total; 885 autogenerated symbols remain.**

The custom-object block is now complete through `ShowCustomObjectEditor` at `005343c0`, including its full preview window and modeless-dialog procedures, command routing, search edit subclass, toggle-button subclass, disabled-control tooltip forwarding, owner drawing, DPI changes, and teardown. The following overworld block is named through level-tile tooltip initialization at `00536650`: graphics-source table setup, bidirectional mode-aware tile-index conversion, Layer 1 and sprite-grid coordinate decoding, directional path-point helpers, editable level/event combos, reveal-event list parsing, and reveal source/destination table updates. Confidence is recorded in the live decompiler comments. Latest verified coverage is **3,077 named functions out of 3,912 total; 835 autogenerated symbols remain.**

The overworld path/warp block is now named through special-destination table handling at `0053abd0`. It distinguishes 128 configurable path links, 256 warp/exit links, reciprocal versus one-way destinations, stale-link removal, map-relative coordinates, the two-click exit-link workflow, level-tile property entry points, and Mario/Luigi start-position conversion. `OverworldEndpointRecord` is recovered as an exact packed five-byte structure (`ushort x`, `ushort y`, `byte submap`) and applied to the 128-entry source, return, and resolved endpoint tables at `00b13ee0`, `00b20760`, and `00b1415b`. Latest verified coverage is **3,108 named functions out of 3,912 total; 804 autogenerated symbols remain.**

The overworld text editors are now named through the next 24-column decoder at `0053e040`. The level-name editor round-trips 256 fixed 19-byte names through display-definition tables and backslash-hex escapes, renders selectable font sheets, and provides commit/rollback behavior. The message-box editor handles 512 fixed 144-byte records (two per level), each organized as eight rows of 18 glyphs, with word wrapping, line compaction, font preview, list synchronization, and transactional rollback. `OverworldMessage` is an exact 144-byte structure and is applied as a 512-entry array at `00b20a30`. Latest verified coverage is **3,141 named functions out of 3,912 total; 771 autogenerated symbols remain.**

The seven-record boss-sequence text editor and following overworld undo core are now named through `PushOverworldUndoRecord` at `0053fb60`. `BossSequenceMessage` is an exact 192-byte structure representing eight rows of 24 encoded glyphs and is applied at `00ca7fc0`. The undo system owns selectively captured Layer 1, Layer 2, sprite, event, path, remap, and auxiliary state; supports shared snapshot buffers between adjacent nodes; transfers ownership flags before trimming or freeing nodes; serializes the 16-byte Map16-remap linked list; and reconstructs 255 dynamic event-array allocations. `OverworldUndoRecord` is recovered as an exact 32-byte doubly linked node and applied to the oldest/current undo globals. Latest verified coverage is **3,161 named functions out of 3,912 total; 751 autogenerated symbols remain.**

Undo restoration and the first overworld rendering/ExAnimation block are now named through `CalculateOverworldAnimationTimerInterval` at `00542870`. The typed undo record is applied component-by-component, including path/warp tables, event reveal metadata, sprite tables, dynamic event arrays, and remap-list reconstruction. Rendering helpers distinguish raw indexed 8x8 tiles, cached tile-atlas compositing, four-quadrant 16x16 Map16 rendering, linked overlay records, transparency, half blending, inversion, and RGB-channel highlighting. The ExAnimation pass separates built-in tile rotation, custom trigger/frame evaluation, destination ownership marking, speed divisors, and legacy timer intervals. Latest verified coverage is **3,179 named functions out of 3,912 total; 733 autogenerated symbols remain.**

The remaining overworld animation and sidecar-loading block is now named through `LoadCustomOverworldSpriteSidecar` at `005438a0`. This pass covers animated palette colors, built-in palette cycling, mode-specific palette caches, palette import/export, animation timer lifecycle, Ctrl+Shift navigation from graphics/palette ownership markers into the ExAnimation editor, trigger initialization, graphics loading for the overworld/title/credits/Layer-3 editor modes, and first-frame cache construction. `.ovpathtxt` maps up to 512 path tiles into `.ovpathbmp` blocks; `.ovssc` supplies custom overworld sprite descriptions, point-based render definitions, bounding metadata, tile-source maps, and palette-source maps. Latest verified coverage is **3,197 named functions out of 3,912 total; 715 autogenerated symbols remain.**

The Rust overworld attribution path now reconstructs those Ctrl+Shift destinations from the same
typed graphics and palette transfers used by the live preview. It visits at most 32 local records
and then 32 global records, lets later writers replace earlier markers, and omits either domain
when its per-map feature switch is disabled. Thus an overlap navigates to the record that actually
wins in the rendered cache. The palette grid and the rendered 8x8 graphics sheet accept Ctrl+Shift
with or without Alt; unowned destinations do nothing. Local owners open the editable map domain,
while global owners select the exact record in an explicitly read-only global view so they cannot
be accidentally applied through the local-overworld transaction.

The per-submap animation controls have now been separated into their two original storage
families. `004B3CB0` decodes seven bytes at runtime `$00B09668..$00B0966E`: inverted bit `$10`
enables submap-local ExAnimation, `$20` global ExAnimation, `$40` original animated tiles, and
`$80` the original level-dot palette cycle; `004B3E80` reconstructs those four bits while
preserving each low nibble. Original lightning is independent. `AdvanceBuiltInOverworldPaletteAnimation`
at `005429D0` snapshots the byte at `$0084AA1E`, tests its high bit for the current map, then shifts
left across exactly seven maps; a clear bit enables lightning. Its ROM loader reads that byte from
revision-descriptor field `$904`, while the seven feature bytes are reached through the installed
overworld-animation runtime selected by descriptor fields `$91C` and `$878`. The pristine mask is
`$F7`, enabling lightning only for native submap four. The Rust preview now has a lossless
two-source semantic model and exact option gates; installed descriptor-relative loading and saving
remain the next boundary.

The installed global-overworld preview now reuses the independently authenticated expanded
ExAnimation split pointer (`LoadGlobalExAnimationData`, bank operand at runtime `+$5C`, low word at
`+$65`). The overworld initialization at `005425D5` assigns local records the first `$20` slots and
global records the following `$20`; the driver at `005426D3` completes the local domain before the
global domain. The Rust materializer consequently caps both domains at 32 records, preserves their
independent source settings and trigger cursors, applies the per-map local/global switches, and
runs global records last when both domains own the same graphics or palette destination.

The descriptor-relative per-map option path is now resolved to concrete SMW-US revision-0 ROM
locations. In the live descriptor at `$005E9DE8`, field `+$878` is physical `$020286`, so the
stable hook operand is logical `$020087`; field `+$91C` is physical `$0026E3`, giving logical
installation marker `$0024E3`; and field `+$904` is physical `$027909`, giving logical lightning
operand `$027709`. `LoadOverworldAnimationFeatureOptions` at `004B3EF0` requires opcode `$22` at
the marker, follows the hook operand, then follows the long pointer at installed runtime `+$4A`
to exactly seven feature bytes. A pristine ROM has no hook and therefore synthesizes seven zero
bytes, while its lightning operand contains `$F7`. Rust now models that exact marker-gated chain,
loads installed options into the live preview, retains the lightning byte's unused low bit,
protects the hook/operands/table from allocation, supplies atomic checksum-repaired
save/reopen/undo I/O, and exposes all five controls per map in the native overworld Animation
panel. The four
runtime-backed controls remain disabled when the marker is absent, while the independent
lightning operand remains editable. A native commit first stages any complete-overworld payload
mutation on a private image, writes both option sources and the checksum there, and publishes one
revision-bound mutation; this prevents either staged domain from overwriting the other. Installing
the missing overworld runtime for a nonzero feature byte remains a separate prerequisite rather
than being guessed from the level ExAnimation runtime.

`InstallOverworldAnimationRuntime` at `004B2440` is now bound to a complete pristine-ROM byte
differential from Lunar Magic 3.63. The ordinary LoROM installation publishes an exact `$C20`
runtime, `$15` auxiliary owner, and seven-byte option owner, with six fixed writes, 25 explicit
relocations, and the 108-entry local-word relocation table. The differential also corrected the
auxiliary model: its seven three-byte entries are mutable submap ExAnimation pointers, not
immutable padding. `FF 00 00` is the exact empty sentinel; a populated entry must resolve to the
payload start of a valid RATS-owned compact animation block. Rust now authenticates and exposes
that complete owner chain, and its mandatory LM 3.63 test compares every core-owner and fixed-write
byte before replaying the authentic populated-submap pointer.

The following overworld Map16-remap and event-render block is now named through `RefreshOverworldEventTileModel` at `0055a380`. The non-event remap path includes node/group insertion, selection, copy placement, constrained relocation, and deletion. Event tiles can be transferred between group-array and linked-node storage, inserted or copied at map coordinates, moved in Z order, and deleted with active indices repaired. Selection helpers cover Layer 1 and Layer 2 rectangles. The render-state layer applies individual and 6x6/2x2 event footprints, constructs per-tile render-link lists, performs reveal-state tile substitutions, preserves active selection flags, swaps editor/render buffers, and redraws only cells whose saved snapshots changed. Comments record high or medium confidence according to structure fields, flags, strings, memory layouts, and call-site evidence. Latest verified coverage is **3,398 named functions out of 3,912 total; 514 autogenerated symbols remain.**

Event navigation and the overworld sprite-editing/UI core are now named through the transition into `SpritePlacementWindowProc` at `00574f80`. Sprite selection, placement, Layer 1/Layer 2 editing, clipboard formats, toolbar/rebar construction, mode transitions, Direct3D lifecycle, central command routing, MDI procedures, and editor lifecycle are covered. Shared dialog and SNES color infrastructure, `Color V2`/`Color Row V2`, screen sampling, and RGB/OKLab interpolation are identified. The level-object properties dialog and standalone palette editor are complete, each with mask rendering, transactional rollback, and its own 32-entry tagged undo history. The palette editor additionally has page selection, full dialog command/mouse handling, custom level versus overworld palette storage, multi-page color propagation, ExAnimation navigation, modeless lifecycle, and exact sRGB/OKLab conversion helpers. The following sprite-placement block begins with preview-DIB teardown and case-insensitive description search with forward/reverse wraparound; its complete window procedure is also named. Latest verified coverage is **3,601 named functions out of 3,912 total; 311 autogenerated symbols remain.**

The sprite-placement internals through `00576ec0` are now named and annotated. This includes `.mw2` placement and `.mwt` description sidecar loading/saving, category/list resource population, preview DIB composition and immediate painting, graphics-slot requirement text, search overlays, hover handling, main status-bar creation, DPI-scaled command icon strips, level-list keyboard subclassing, case-insensitive description search, custom placement append, and synchronized deletion of placement/description records. Latest verified coverage is **3,649 named functions out of 3,912 total; 263 autogenerated symbols remain.**

The `.mw2` parser starts after a retained one-byte stream header and uses the same four 256-entry
sprite-length tables as native level records. Each placement contains one or more complete records;
bit zero on a record after the first begins the next placement. A single `FF` byte terminates the
stream. Both `.mw2` and `.mwt` buffers are capped at `0x8000` bytes. `.mwt` accepts an optional
UTF-8 BOM and newline-delimited descriptions; append, delete, and move operations keep description
lines synchronized with whole variable-width placement ranges.

The remaining sprite-placement list model and preview UI through `0057b5c0` are now named and annotated. Recovered behavior includes synchronized record reordering, external and built-in SP-slot compatibility checks, graphics filtering, category merging, list-icon rasterization and centered 32x32 cropping, list and preview tooltips, owner-draw row sizing, zoom menus and scaling, monitor/DPI-aware dialog layouts, deferred refresh, the placement command dispatcher, and the complete preview child window procedure. Latest verified coverage is **3,709 named functions out of 3,912 total; 203 autogenerated symbols remain.**

The lowest-address residual-symbol audit and the UI/color-quantization tail through `0057ed30` are now named and annotated. This pass recovered all remaining LZ2 back-reference/fill primitives and its bounded 64 KiB decoder, relative level-object tile-list rendering, contiguous object-list snapshots, Direct3D adapter/device/reset/upload helpers, embedded level-editor graphics resources, Map16 and 8x8-viewer selection/grid rendering, overworld zoom adjustment, the complete Add Sprites dialog and tooltip infrastructure, and the variance-based Wu RGB quantizer used to emit rounded SNES BGR555 palettes. The Rust quantizer mirrors the five 33³ moment arrays (four wrapping 32-bit integer arrays and one single-precision squared-moment array), Lunar Magic's red-plane/green-area/blue-line accumulation order, single-precision cut scoring and variance, truncating cluster averages, and bit-2 SNES channel rounding. Latest verified coverage is **3,771 named functions out of 3,912 total; 141 autogenerated symbols remain.**

The statically linked CRT tail through `00589416` is now semantically separated from application code and annotated. Recovered library behavior includes HTML Help delay loading, secure/unbounded fread and fwrite cores, FILE and descriptor locking/unwind thunks, fclose and recalloc, ANSI/wide stat and mkdir, Win32 error translation, CRT exit and runtime-error paths, printf formatting, pointer encoding, per-thread data creation, text/Unicode descriptor reads, time conversion, ANSI stream-mode parsing, FILE allocation, SEH local unwind, and CRT signal dispatch. Latest verified coverage is **3,858 named functions out of 3,912 total; 54 autogenerated symbols remain.**

The final CRT/math tail is now named and annotated. It covers on-exit registration and terminate handling, CRT error-mode and message-box dispatch, multibyte/code-page initialization, timezone/environment storage, ANSI and wide descriptor-open cores, process-environment mutation, x87/SSE2 `pow` implementations and exceptional cases, x87 math-exception forwarding, and decimal-string/long-double conversion. This completed the original 3,912-entry symbol pass; the later function-boundary audit at the top of this ledger supersedes that historical count.

## Final symbol audit

- Live program: `Lunar Magic.exe` in the Ghidra session exposed on TCP port 8089.
- Internal functions enumerated: 4,027; imported/external functions: 386; Ghidra total: 4,413.
- Functions retaining a `FUN_...` name: 0.
- Every formerly autogenerated function received a semantic symbol and a decompiler comment recording behavior and confidence.
- Application code, compiler-generated cleanup/thunk entries, and statically linked CRT/math routines are named distinctly so clean-room reimplementation work can separate product behavior from library/runtime behavior.
- Recovered application data types and globals include level object/sprite nodes, multiple native clipboard headers, overworld endpoint/message/undo records, Map16 remap nodes, palette history records, sidecar streams, and editor backing buffers. Exact layouts are used where field offsets and sizes were proven; uncertain fields remain explicitly marked rather than guessed.

The adjacent overworld text pipelines are now separated and annotated as level-name, message-box, and boss-sequence storage. Expanded message text supports three pointer-addressed banks; boss-sequence text uses 56 fixed records. The overworld sprite subsystem is also named through its top-level load/save orchestration, including the seven-map custom-sprite stream, 24-record per-map limit, variable record-size table, built-in sprite tables, and ROM allocation lifecycle. Top-level overworld load/save and selective text-save entry points now have visible semantic symbols in Ghidra's Functions window. The following palette and expanded layer-tilemap initialization routines are named as well.

Title-screen, credits, and overworld Layer 3 graphics paths are now distinguished. This includes title-screen saving, title and credits graphics loading, legacy and expanded credits-row decoding, credits tilemap deduplication/serialization, and overworld Layer 3 graphics/tilemap loading.

The LMSW emulator-plugin integration block through `004c2ed0` is fully named and annotated. Recovered behavior includes DLL export resolution (decorated and undecorated APIs), lifecycle management, ROM and sprite transfer, pause-reason aggregation, single-frame stepping, editor scrolling, viewport backing-store capture/restore, overlay rendering, and level-load notifications. Added the 16-byte `LmswViewportRect` structure, applied it to capture/restore prototypes, recovered several scalar prototypes, and named the principal LMSW state globals and drawing/pause/step export pointers.

The following level-editor sprite rendering and manipulation subsystem through `004ce5e0` is now substantially annotated. Recovered the per-cell linked tile renderer, signed-offset and screen-wrapping logic, 256-entry standard-sprite render dispatch table, custom metadata rendering path, entrance rendering and packed entrance-table synchronization, sprite stream parser/serializer, list sorting and insertion, selection deletion, dirty-cell invalidation, and group movement/clamping.

A direct-ROM load/export oracle now separates legacy ordering from edit gestures: level `$105`'s
first record was changed to screen `$1F` and its second to screen `$00` without invoking Lunar
Magic's editor. Lunar Magic 3.63 exported the complete list in stable screen order, including the
original same-screen priority. This binds legacy semantic serialization to the recovered list-sort
path; expanded sorting remains orientation-aware because vertical levels add the recovered low-Y
nibble tie-breaker.

A matching direct-ROM expanded oracle writes two level `$105` records in descending screen and
resolved upper-Y order, then invokes export without an edit gesture. Lunar Magic 3.63 restores the
same stable horizontal comparator and minimum `$FF vv` transitions used after positional edits.
Together with the retained vertical oracle, this proves expanded ordering is a semantic
load/serialization invariant and that vertical modes alone add the low-Y-nibble key.

The complete byte-sized standard-sprite preview domain is now classified against that dispatch
table. IDs `$29`, `$30`, `$EE`, `$F0`, and `$F1` deliberately select Lunar Magic's native
empty/default handler; IDs `$F6`–`$FF` are reserved for SSC custom-display bookkeeping; every other
ID selects recovered built-in artwork. The Rust renderer exhaustively tests all 256 IDs against
this partition, and the native editor leaves intentional empty handlers artwork-free while
retaining a visible diagnostic when required custom-display data cannot be resolved.

The entrance synchronization path is now tied to the binary MWL boundary. `SynchronizeEntranceNodeData` (`004ccda0`) projects 40-byte editor nodes from packed main, midway, and secondary-exit state; `RebuildLevelEntranceNodes` (`004cd7e0`) creates one main node, a conditional midway node, and secondary nodes targeting the current level. `ExportBinaryMwlLevelFile` proves that the 64-byte level-header section owns main-entrance bytes at offsets `2`-`6`, `14`, and `15`, and midway-specific bytes at offsets `9`-`12`. The Rust `MwlLevelHeaderSection` exposes these as lossless typed records and the native MWL editor can modify them without normalizing the other 53 bytes. A reciprocal Wine oracle proves Lunar Magic 3.63 imports and re-exports a changed main position exactly; it also proves that midway-only bytes are normalized to zero when the destination ROM lacks Lunar Magic's separate-midway runtime.

The complete original entrance-dialog transaction is now bound as well. Editor command `$2524`
creates resource `$03F0` with callback `$00410440`; callback initialization populates controls
through `$00410DA6`, while IDOK packs the edited globals through `$0041097E`, rebuilds entrance
nodes, and marks the level modified. IDCANCEL calls `EndDialog` without entering that apply path.
The live `original_main_midway_dialog_applies_reopens_and_cancels_losslessly` gate exercises both
branches, including the separate-midway enable transition, and binds the resulting main bytes
`54 13 B7 1A C0 00 5A` and midway bytes `00 E9 0A 4B` to Lunar Magic's own MWL export.

Secondary-exit editor command `$2525` creates dialog resource `$03F1` with callback `$00411790`.
`PopulateSecondaryExitSelector` (`$00410DB0`) publishes all `$2000` slots;
`LoadSelectedSecondaryExitIntoDialog` (`$00410F20`) decodes the six planes into the level,
position, FG/BG, action, and overworld controls; and `ApplyDialogToSelectedSecondaryExit`
(`$004112B0`) packs them back. Clear Slot calls `$00473B80`. Clear All first obtains the exact
“Really clear all slots?” confirmation and calls `$00473B60` only when the result is not IDNO.
The outer command keeps a `$C000`-byte backup and restores it when the dialog does not return IDOK.
The live dialog gate binds all of those paths. It additionally proves that saving a completely
empty installed table keeps the first four fixed planes, zeroes them, writes null operands for
planes four and five, and owns no RATS payload for those null tail planes.

Comparing all 512 pristine MWL exports against the source ROM locates the four vanilla
main-entrance planes exactly at headerless PC offsets `$2F000`, `$2F200`, `$2F400`, and `$2F600`.
They correspond to MWL header offsets `2`-`5` for position, vertical settings, screen/method, and
level-mode/screen respectively. The Rust project layer now loads and transactionally saves these
planes with checksum repair and undo support, and the built-in pristine-SMW GUI exposes the record.

The optional separate-midway path is now fully recovered and implemented.
`LoadCurrentSecondaryExitRuntimeState` (`00473130`) follows the installed JSL helper and reads four
`$200`-byte planes; the save path at `00473280` requires the allocation to be exactly `$800` bytes
before writing them. A dynamic Lunar Magic installation maps MWL header offsets `9`-`12` to flags,
position, additional flags, and high position at plane offsets `+$000`, `+$200`, `+$400`, and
`+$600`. The fixed hook is at headerless PC `$2D9E3`; its `$D0`-byte RATS-owned helper contains four
table operands and one self-call relocation. Rust validates the hook, both RATS owners, helper
opcodes/version marker, all plane pointers, and exact table length before editing. Its pristine
installer composes Lfix3, the helper, table, selected-level `$20` enable flag, checksum repair, and
all relocations in one undoable transaction. Reciprocal Wine oracles prove both updating a Lunar
Magic-installed table and opening/exporting a Rust-installed table preserve the exact midway
fields.

Added `EditorRenderTileNode` (12 bytes) and `LevelSpriteNode` (40-byte allocation with 34 bytes of currently recovered fields), applied `LevelSpriteNode` prototypes to the parser, serializer, sorter, and destructor, and typed/named the active sprite-list head, the 0x3800-entry render-list array, current rendered sprite, and 256-entry renderer dispatch table. Fixed-pattern sprite handlers whose exact gameplay identity is not yet proven use conservative tile-pattern names and medium-confidence annotations rather than speculative sprite names.

The level-sprite selection, clipboard, undo, and placement-preview layer through `004d0aa0` is now named and annotated. Recovered behavior includes render-node hit testing, point and rectangle selection, additive/removal operations, select-all filtering, group drag clamping, duplication, forward/backward z-order changes, undo snapshot export/import, the `Lunar Magic Sprites V7` clipboard format, compatibility-mode sprite-ID translation, clipboard paste placement, and temporary sprite-placement preview rendering.

Typed and named the selected-node pointer array/count, 0x3800 dirty-cell bitmap, temporary clipboard clone array/count, and several related buffers. Applied recovered `LevelSpriteNode` prototypes to selection, hit-test, undo, and clipboard routines. The separate renderer beginning at `004d09f0` was subsequently confirmed as the overworld sprite display grid and is described below.

The 0x2000-cell renderer is now confirmed as the overworld sprite display grid and is named through full rendering, custom display definitions, per-cell hit testing, insertion, property transfer, cache capture, and cleanup. Added the recovered `OverworldSpriteRenderNode` layout and typed/named the 8192-entry render-grid array.

The adjacent display-definition text parser is annotated through tokenization, whitespace/comment handling, diagnostics, record allocation, duplicate removal, sorted indexes, and binary lookup in both directions. Added the 132-byte `DisplayDefinitionRecord` type (24-byte key, 24-value integer sequence, lengths, and source ordinal), and typed/named its source cursor, line counter, record array, and both sorted indexes. The exact external file extension remains deliberately unspecified until established by a calling path.

External-tool notification callbacks through `004d37d0` are also named for ROM-open, level-save event codes 1/3, level deletion, Map16 changes, and overworld saves.

The `usertoolbar.txt` subsystem through `004d4a50` is now named and annotated. Recovered functionality includes line/token parsing, keyword dispatch, escaped tooltip text, `%1`-`%9` ROM path/level/window substitutions, toolbar item finalization, bitmap-strip and executable-icon loading, DPI image-list rebuilding, external-tool definition allocation, launched-process tracking, event subscriptions, and complete shutdown cleanup.

Added `UserToolbarButtonDescriptor` (12-byte allocation; 9 bytes currently recovered) and `ExternalToolDefinition` (88 bytes), and typed/named the dynamic toolbar descriptor and external-tool definition arrays with their count/capacity globals and current icon size.

The external-tool runtime following the parser is now named through toolbar-window creation/subclassing, enable/check synchronization, keyboard-shortcut dispatch, command expansion and process launch, ROM-open auto-launch, tooltip delivery, and interprocess editor requests. The language-module path helpers and resource loader are named as well.

The localization and Unicode compatibility layer through `004d9d60` is now annotated. Recovered behavior includes language-DLL checksum validation, installed-language enumeration and OS-language auto-detection, localized modal/modeless dialog selection, mapped binary resources, right-to-left window/bitmap/icon mirroring, UTF-8/UTF-16/code-point conversion, escaped multi-string separators, ANSI compatibility conversions, locale-aware sort-key generation, Unicode ShellMessageBox fallback, UTF-8 window creation/class registration, and UTF-8 list-box text retrieval. The overlapping entry at `004d8805` is conservatively labeled as an alternate UTF-8 serialization entry pending function-boundary correction.

Direct port-8089 recovery closes the localization-selection state machine. `004D7940` adds
`(Default)` English and accepts only `sysLMLanguage\\*.dll` modules with resource `$0DB7` magic
`$C001BABE` plus at most `$410` bytes of resource `$0DB6` metadata. `004D7360` keeps persisted
`(AutoDetect)` distinct from `(Default)`, compares as many as 64 preferred UI-language tags first
exactly and then by primary language, and falls back to English without loading a module.
`004DB810` bounds the Windows list to `$600` UTF-16 units and uses `004DB640` for the legacy
full-tag/primary-tag fallback. `004D7010` validates the decoded pre-trailer bytes against the dword
at `file_size - $38`. The retained contract is in
`docs/oracle-work/lm363/localization-auto-detect/`.

Unicode-compatible Win32 wrappers are additionally named through `004db810`: combo-box text retrieval, UTF-8 virtual-key lookup, UTF-8 activation-context creation, window/dialog text setters and getters, dynamic preferred-UI-language APIs, the legacy language-ID fallback table, and UTF-8 preferred-language multi-string production.

The UTF-8 Win32 adaptation layer is now named through `004e0680`. It covers ShellExecute and CreateProcess, common open/save dialogs, UTF-8 file creation/deletion/copying and attributes, current/short/full/module/executable paths, drag-and-drop filenames, file enumeration, and UTF-8 LoadLibrary variants. Added the 1,140-byte `Win32FindDataUtf8` structure with 1,040-byte primary and 56-byte alternate UTF-8 filename buffers, and applied it to the first/next enumeration prototypes. The entry at `004ddd6a` overlaps the short-path routine and remains explicitly marked as a medium-confidence boundary.

The remaining UTF-8 compatibility wrappers through `004e4b30` are now named: image loading, command-line tail extraction, UTF-8 `fopen`, directory creation and file status, UTF-8 registry strings including multi-strings, menu item access, text measurement/drawing, text-bearing window messages, and HTML Help with a short-path fallback. Analysis then enters Lunar Magic graphics code: cached graphics-context cleanup, dialog hexadecimal helpers, decoding 1,024 SNES 4bpp planar tiles into chunky pixel indices, and initialization of a 1,024-entry identity tile-remap table. The remap array, initialization flag, and cache-state scalar are typed and named.

The Map16 file and editor subsystem through `004e7780` is now named and annotated. Recovered file formats include per-page `Map16Page.bin`/`Map16PageG.bin`, complete foreground `Map16FG.bin`/`Map16FGG.bin`, complete background `Map16BG.bin`, and the 0x1C000-byte sprite `.s16` sidecar. The SNES-file importer consumes a 0x8000-byte 4bpp graphics set and 0x800-byte screen map, optionally imports a palette, remaps tile numbers, deduplicates 16x16 definitions, and installs them into blank Map16 slots.

`ImportCurrentMap16PageFiles`/`ExportCurrentMap16PageFiles` at `004E5A60`/`004E5B60` prove the
legacy page-pair plane names directly: `Map16Page.bin` transfers `0x800` definition bytes from the
four-subtile buffer, and foreground pages additionally transfer `0x200` Acts-Like bytes through
`Map16PageG.bin`. The `G` suffix does not identify the graphics plane. Rust's paired worker now
binds those exact names, lengths, and planes instead of the previously reversed interpretation.

The adjacent complete legacy functions establish the same naming rule and exact namespace sizes.
`ImportAllForegroundMap16Files`/`ExportAllForegroundMap16Files` at `004E5C60`/`004E5D40`
transfer `0x40000` definition bytes through `Map16FG.bin` and `0x10000` Acts-Like bytes through
`Map16FGG.bin`; `ImportAllBackgroundMap16File`/`ExportAllBackgroundMap16File` at
`004E5E20`/`004E5EB0` transfer `0x40000` definition bytes through `Map16BG.bin`. Rust exposes all
three exact file families as bounded, revision-bound native actions, publishes the foreground pair
atomically, preserves protected built-in definition words on import, and canonicalizes background
Acts-Like values to zero.

All three import functions call the same `fread`-style wrapper with one fixed-size element. A short
file therefore overwrites only its available prefix and retains the current buffer suffix; trailing
bytes are ignored. For page and foreground pairs, a missing `G` companion occurs after the
definition read and therefore leaves the definition prefix applied while retaining the complete
current Acts-Like plane. Rust matches those final semantics with bounded prefix reads and one
revision-checked staged edit, retaining failure atomicity without changing the observable result.

`LoadM16Map16SidecarData` reads one fixed `0x2000`-byte `.m16` block. The `.s16` loader first
zeros its full `0x1C000`-byte buffer and then reads any available prefix up to that capacity.
`WriteS16SpriteMap16SidecarFile` scans the buffer as `0x7000` little-endian dwords from the end,
keeps through the last nonzero entry, rounds the byte length upward to an `0x800` boundary, and
writes a minimum `0x800` bytes for the all-zero case. The raw dword fields remain intentionally
uninterpreted until their consumers provide stronger evidence.

Map16 rendering is traced from individual flipped 8x8 SNES tile descriptors through the cached 256x256 page bitmap and selected-tile previews. `SelectMap16TileForEditing` at `004e6cd0`, `CopySelectedMap16TileToClipboard` at `004e6dd0`, and `PasteSelectedMap16TileFromClipboard` at `004e6eb0` establish the single-tile clipboard boundary. Copy registers the singular custom format name `Lunar Magic 16x16 Tile` and publishes exactly ten bytes: four little-endian subtile descriptors in top-left, top-right, bottom-left, bottom-right order followed by the little-endian Acts Like value. Paste accepts a global allocation of at least ten bytes and consumes its first ten. The exact `Map16TileClipboardRecord` and the Windows bridge now reproduce that format across the standalone page, complete-set, and ROM Map16 editors while retaining the portable Unicode fallback. The main editor window procedure, keyboard/control handlers, page navigation, import/export shortcuts, attribute flipping, acts-like-cycle detection, selection/paste paths, and cache lifecycle are named. The current page, selected subtiles, acts-like value, selected absolute tile index, and active-selection flag are typed and named.

The retained isolated-Wine `map16-single-tile-clipboard/oracle.tsv` binds those original entry points
to the registered cross-process boundary. Copying vanilla tile `$000` publishes exactly ten bytes,
`70 1C 72 1C 71 1C 73 1C 00 00`. Publishing the deliberately asymmetric record
`23 01 67 45 AB 89 EF CD 57 13`, invoking original paste, and copying again returns the same record
byte-for-byte. This proves both word ordering and Acts Like placement independently of Rust.

The retained `map16-editor-interaction/oracle.tsv` closes the complementary live GUI boundary.
Command `$232F` opens the modeless dialog; its page field plus actual mouse-move-backed drag selects
page `$02`, tile `$200`. Four subtile replacements, Acts Like, palette, priority, X flip, and Y flip
create exactly nine original history records. Nine Undo actions restore all original visible values,
and nine Redo actions restore all modified values. The flipped quadrant order proves that the
attribute controls operate on the selected definition rather than merely changing form state.

The modeless editor's later import/export dispatcher is recovered independently at
`00501550`. Commands `$2266/$2267` export/import the selected compact `.map16` range, while
`$2268/$2269` export/import the complete `AllMap16.map16` container. `HandleMap16RenderWindow` at
`005008A0` bounds the key range then passes `virtual_key - $1B` into the table consumed by
`HandleMap16EditorKeyCommand` at `004FFEF0`; the resulting entries map unmodified F2 to selected
export and F3 with any modifier state to selected import. Rust routes those fixed keys through the same bounded,
revision-bound file workers as the visible selected-range buttons and consumes each accepted key
event once.

The same recovered table maps actual F9 through case `$0F`: unmodified F9 calls
`CommitAllMap16ChangesToRom`, Ctrl+F9 writes the `.m16` sidecar after confirmation, and
Ctrl+Shift+F9 writes the `.s16` sidecar after confirmation. The native installed-ROM Map16 editor
routes unmodified F9 through its existing revision-checked complete-set commit transaction and the
two modified chords through distinct confirmation-backed sidecar exports. Both names replace the
opened ROM's extension. The `.m16` export snapshots exactly `0x2000` bytes; `.s16` snapshots the
full `0x1C000`-byte working buffer and applies the recovered last-nonzero, `0x800`-rounding, and
minimum-`0x800` canonical writer. When the standalone editor has a matching live sidecar document,
that staged document is the exported buffer; otherwise the ROM-associated buffer loaded on editor
open is used. Missing siblings retain the original default `.m16` and zero `.s16` buffers. Loading
and atomic create/replace persistence are bounded and revision-tagged, and malformed or non-regular
targets cannot partially publish an export.

Cases `$03/$04` are actual Up/Down. Their assembly reads the current row-aligned Map16 position:
Up aligns to the current page boundary or subtracts `$10` rows when already aligned, while Down
adds `$10` rows; both call `NavigateToMap16Page` at `$004FAD60`, which clamps the lower boundary,
scrolls to the aligned page, and updates the hexadecimal page status. Neither branch tests a
modifier. The native editor therefore consumes Up/Down with every modifier state and moves one
bounded page while retaining the selected within-page tile.

Case `$0E` is actual F8. Every modifier state except simultaneous Ctrl+Alt sends `BM_CLICK` to the
16×16-grid control; Ctrl+Alt (with or without Shift) instead toggles `DAT_005E553C` between the
original white and black grid colors and refreshes the view. `DrawMap16PageBoundaryGrid` at
`$004F9790` draws the configured color at each 16-pixel boundary. The native rendered-page canvas
now exposes the same initially hidden grid, fixed F8 gesture, independent color gesture, and visible
white/black control while keeping the selected-tile outline above the grid.

Table case `$08` is actual `V` and dispatches paste command `$2276` whenever Ctrl is down; it does
not reject simultaneous Shift or Alt. The native editor now consumes those exact Ctrl+V variants
and routes them through the same request-captured ROM revision, staged revision, and Map16 address
used by its visible Paste Tile action. Non-Ctrl V remains unconsumed.

Cases `$07/$09/$0A` are actual Numpad 0/+/−. They consume the keys in every modifier state but only
dispatch zoom commands `$2447/$2448/$2449` while Ctrl is down. The command jump table proves reset
uses the current system-DPI scale, while plus/minus call `SetMap16ZoomPercent` with `+100/-100` and
the callee clamps the result to 100–5000 percent. Since egui already applies system DPI outside the
logical canvas, the native adapter represents system reset as 100 logical percent; it otherwise
retains the exact step and bounds. The rendered texture remains 256×256 pixels, while its scrollable
presentation, grid, selection outline, and proportional hit testing share the selected scale.

Case `$0B` is actual F1. With Shift down it performs no action. Without Shift, Ctrl enters the
localized warning path that toggles `DAT_00E27871`, refreshes the Map16 view, and marks the editor
dirty; without Ctrl it clicks the page-number control whose tooltip is `Display page numbers.` Alt
is not tested in either branch. The native editor now matches this gesture partition: F1 toggles a
`Page 0xX` canvas overlay, Ctrl+F1 opens an explicit confirmation before pages `$00–$01` become
manually editable, and every Shift combination is inert. Lock state gates subtile, Acts-Like, and
typed clipboard mutations without weakening the existing protected-word import rules.

Table case `$05` is actual Insert and reaches the SNES graphics-set/screen-map importer only when
Ctrl, Shift, Alt, and F1 are all simultaneously down. The native installed-ROM editor now consumes
that exact chord and invokes its existing revision-bound `Load SNES tileset…` sequence, preserving
the same active-worker and foreground-page gates as the visible action. Insert without the complete
chord remains non-mutating.

The separate Map16 tile-selector/viewer subsystem through `004e99c0` is now named. It consists of an outer selector window, a scrollable 256x256 tile-view child, and a status bar. Recovered behavior includes DPI-aware percentage scaling, client/outer size calculation, horizontal and vertical scroll state, mouse-wheel page motion, hover and primary/secondary selection highlighting, keyboard page navigation, foreground-page unlocking, palette-context changes, and top-down 32-bit DIB cache creation/rendering/cleanup. Typed and named the current/maximum selector page, selected and hovered absolute tile numbers, palette context, and backing pixel pointer.

The outer Layer 1 selector creator and the beginning of the main level-editor presentation layer through `004eac40` are now named. This includes renderer/file-error reporting, status-bar sizing and DPI handling, horizontal/vertical level-editor scroll state, backing-cache and auxiliary-buffer cleanup, and the toolbar icon system. The toolbar uses a 24-entry command table with parallel enabled/disabled icon arrays, supports an external `Lunar Magic.ff5` bitmap, compressed and built-in fallbacks, per-window DPI scaling, right-to-left mirroring, and a separately rebuilt alternate mode cache.

The level-editor modification and selected-tile transaction layer through `004ebb10` is now annotated. The dirty-state setter drives command `0x2261` and save/discard/cancel prompting. Tile selection uses four 0x13D00-entry planes of per-tile state, rectangle rasterization, cached bounds/counts, temporary Map16 definition and acts-like snapshots, bounded translation, drag updates, and placement at a requested grid point. Live and temporary definitions are swapped so overwritten tiles remain recoverable during movement. Typed and named both 324,608-byte selection-state arrays, selected-tile count, nonempty-bounds flag, and drag-active guard. Three following capacity counters are intentionally named by proven mechanics because their exact resource-table identities are not yet established by callers.

The main Map16 rectangle gesture is now bound end to end. `HandleMap16RenderWindow` at `00500850`
routes mouse down through `HandleMap16EditorLeftButtonDown` at `004fbc50`; active selection state 1
moves through `HandleMap16EditorMouseMove` at `004fb750` and
`MoveMap16SelectionAnchorAndRedraw` at `004eb110`; mouse-up or capture loss finalizes at
`004fbb10`. Both axes are snapped to 16-pixel cells, reverse drags are normalized, and
`ShowMap16SelectionDimensions` at `004fb620` reports `abs(endpoint - origin) / 16 + 1`.
`DrawMap16SelectionMarquee` at `004f9340` resets each edge to a one-source-pixel repeating
white/black/black/white phase. The Rust rendered-page selection now reproduces that geometry,
inclusive dimension rule, and pre-scale marquee phase through its full 100–5000% zoom range.

The following palette-allocation engine through `004ece10` is now named. It initializes palette-entry reservation states, optionally propagates a selected color across eight rows, converts RGB to Windows HSL-240 coordinates, builds unique color histograms for 8x8 tiles, performs weighted RGB555 palette selection, and models recurring tile color sets and their subset dependencies. Added the exact 184-byte `PaletteColorSetRecord`, containing up to 16 colors, direct and aggregate weights, source pointers, subset pointers, selection flags, and aggregate total. The final greedy selector maximizes overlap with already chosen colors and aggregate utility while respecting remaining palette capacity.

The bitmap-import orchestration block through `004ef770` is now named and annotated. It extends weighted color sets into available palette rows, marks subset records assigned, maps imported pixels to palette indexes, detects blank and duplicate 8x8 graphics including horizontal and vertical flip equivalents, allocates free graphics slots, assembles or deduplicates 16x16 Map16 entries, commits editable palette changes, and drives the complete bitmap quantization/import pipeline. The occupancy scanner covers all `0x300` graphics slots; tile-map results preserve palette, priority, and flip attributes in the final 16-bit tile words.

`FindNextBlankMap16Tile` at `004ef030` is confirmed directly from its assembly and both callers.
`EDI` points to a caller-owned cursor initialized from `DAT_005e55e4`; the function scans upward to
the exclusive bound in `DAT_009b9964`, skips the reserved index in `DAT_005e55f0`, and accepts an
entry only when all four graphics words are exactly `0x1004`. It returns the accepted index (or the
upper bound on exhaustion) in `EAX` and stores `EAX + 1` back through `EDI`. The sequential caller
at `004ef090` preserves the bitmap's spatial layout in destination strips up to 16 Map16 tiles
wide. It starts each source row's blank search at `strip_base + row * 0x10`, then advances
`strip_base` by `source_height * 0x10` for the next 16-column source strip. This is not a flat
source-order allocation. The deduplicating caller at `004ef2d0` instead reuses an earlier imported
four-word block and advances its global cursor only for a unique block. The import pipeline sets
the upper bound to `0x8000` when the initial cursor is below
`0x8000`; otherwise it rounds the initial cursor down to a `0x1000`-tile boundary and uses the next
boundary as the exclusive limit.

The exhaustion branch is now observed through the complete original UI rather than inferred only
from assembly. With a four-definition 32×32 import beginning at `$8FFF`, Lunar Magic writes the
first definition at `$8FFF`, leaves the remaining three unassigned, preserves the completed
palette/graphics conversion, and opens `Not enough free 16x16 tiles!` with the message `There
weren't enough blank 16x16 tiles remaining to import this bitmap.  Only some of them have been
imported.` The pre/post 524,288-byte definition capture differs at seven bytes, all inside the
single `$8FFF` record, and matches the Rust prefix allocator exactly.

Opaque black exposes a separate exact-match prepass. With a solid black source, Lunar Magic keeps
the existing usable black at row 0/index `$D`, encodes every source pixel as `$D`, and writes a real
tile at graphics `$200`; it does not route the source through transparent index zero. A 32×16
black/red fixture with the generated-color limit reduced to one proves that exact black does not
consume that limit: black still uses row 0/index `$D`, while the one generated median color occupies
row 1/index 1. Maintain Detail retains the same exact-black bypass before its distinct-source
sentinel pass. Rust now preserves free-cell values only for this prepass, marks the claimed entry
without seeding it into later row proposals, and clears every unclaimed modifiable value before the
ordinary allocator.

The leading zero-color candidate in Maintain Detail is not a transparency marker. A 32×16
near-black/red capture with a one-color limit assigns near-black to that candidate, then maps the
opaque pixels to the retained black at row 0/index `$D`; graphics tile `$200` contains index `$D`
throughout instead of zero pixels. Transparency remains a separate source-alpha condition. Rust
therefore materializes a sentinel-selected black in the reduced color table under a nonzero pixel
index before palette-row allocation.

The live 524,288-byte definition workspace at `00777e58` stores each tile's four graphics words in
column-major order (`top-left`, `bottom-left`, `top-right`, `bottom-right`). ROM and Map16 file
definitions remain row-major. Three full-workspace Wine captures distinguish this adapter detail
from allocation behavior: default deduplication at `$8200`, non-deduplicated 2×2 placement at
`$8200/$8201/$8210/$8211`, and priority-enabled allocation from nondefault cursor `$83A5` all match
the Rust reconstruction byte-for-byte.

The executable's initialized option block at `005e55e0` contains a Map16 cursor of `0x8200` at
`005e55e4` and reserved index `0x8000` at `005e55f0`; with the rule above this produces the
exclusive bound `0x9000`. These are persisted dialog preferences rather than universal import
constants—the retained Wine oracle has also demonstrated imports beginning at `0x0200`. The
reimplementation must therefore keep the start cursor and reserved definition distinct from the
currently displayed Map16 page and from the separate `0x00f8` blank **8×8 graphics** tile option.

The special branch in `ImportBitmapAsDeduplicatedMap16Tiles` is also confirmed against
`BuildOccupiedGraphicsTileMap`: when option byte `005e55f8` is enabled and all four referenced 8×8
tiles have no nonzero decoded pixel planes, the output index grid receives `DAT_005e55f0`
directly. No blank Map16 definition is consumed or written for that source block. Sequential mode
does not take this shortcut and materializes the four tile words in an allocated definition.

The preceding blank-8×8 decision is independent. `DeduplicateImportedGraphicsTiles` at `004ee470`
checks option byte `005e55f7`; an all-zero decoded tile takes the configured `DAT_005e55ec` index
with marker bit `$10000000` only when that byte is enabled. `AllocateImportedGraphicsTileSlots` at
`004eee40` consumes the marker without allocating or writing a slot. With the byte disabled, its
ordinary free-slot path allocates the same zero tile and then recomputes the corresponding
`DAT_009b8588` occupancy byte from the encoded planes, leaving it zero. Therefore the later
`005e55f8` Map16 decision recognizes either graphics result as blank. The retained
`map16-bitmap-transparent-blank/oracle.tsv` truth table binds the resulting independent 2×2 option
product to these two original branch addresses.

The native graphics workspace shape is now confirmed both statically and dynamically. Lunar Magic loads eight `$1000`-byte FG/BG slots of `$80` decoded 4bpp tiles each, while `BuildOccupiedGraphicsTileMap` and the bitmap allocator inspect exactly the first six, producing tile numbers `$000..$2ff`. The default allocation globals at `005e55e0` select first tile `$200`, exclusive workspace end `$300`, and blank fallback tile `$0f8`. Thus the four vanilla object-tileset GFX slots occupy `$000..$1ff`; import allocation begins at FG/BG slot 4 and continues through slot 5. The remaining default slot assignments are the `$7f` blank sentinel, so imported pixels in those slots require a concrete GFX/ExGFX assignment before they can be persisted semantically.

A live Wine oracle used the modeless Map16 editor command `$2276` after publishing both `CF_BITMAP` and a normalized positive-height `CF_DIB`. Lunar Magic's clipboard dispatcher tests `CF_BITMAP`, but `ImportClipboardBitmapAsMap16` subsequently obtains `CF_DIB`; a top-down negative DIB height is not rejected and corrupts its unsigned dimension state, explaining earlier automation failures. With a valid 256×256 solid fixture, the preview reported `$400` source 8×8 cells, two converted tiles, one optimized tile, and `$100` available tiles. Accepting it changed exactly occupancy byte `$200` and the planar cache beginning at `0086b7e8 + $4000`, proving that the first new tile is encoded into slot 4. The default Other Options dialog has 8×8 optimization, reuse of existing 8×8 tiles, and 16×16 deduplication/background paste enabled; the Color Options dialog defaults to high-color reduction method 1, priority for exact existing-palette matches and unique colors, and allows modification of colors not marked fixed.

The bitmap-import palette editor and dual-preview UI through `004f3370` is now named and annotated. The options dialog manages quantizer selection, palette-row reservations, fixed colors, color-count limits, priority, and snapshot restoration. Two custom child windows render the original and converted bitmaps with shared DPI-aware sizing and synchronized scroll positions; creation, painting, scrollbar configuration, resizing, and teardown are labeled separately.

A fresh MCP audit of the live Ghidra project fixes the color-option state more precisely.
`InitializePaletteEntryUsageMap` at `004ebb50` uses exact state bits `$01` for an importer-assigned
color, `$02` for a reserved/excluded entry, and `$04` for a preserved reusable entry. It marks
index zero of all eight active 16-color rows reusable, exposes entries 1–8 of rows 0 and 1 as free,
and initially reserves the remaining entries. `HandleBitmapImportOptionsDialog` at `004f15e0`
offers “Median Cut” and “Popularity”, a 1–128 color limit, four priority levels, three whole-row
state buttons, independent reserve/reuse toggles, and transactional restoration of all 128 state
bytes plus the palette snapshot on cancel. The Rust dialog now mirrors those three whole-row
Free/Reusable/Reserved actions for every one of the eight rows in addition to its individual entry
controls. A focused boundary test proves that each action changes exactly 16 entries, is
idempotent, and rejects out-of-range row indexes. Initialized data at `$005e55cc` confirms the related
priority toggles enabled, priority level 3 at `$005e55d0`, maximum 128 colors at `$005e55fc`, and
the recovered default optimization flag block.

The allocation pipeline is row-semantic rather than a single global 4bpp palette.
`BuildTileUniqueColorHistogram` and `FindOrCreatePaletteColorSetRecord` construct weighted unique
color sets for each 8×8 source tile; `BuildPaletteColorSetSubsetLinks` and
`AggregatePaletteColorSetWeights` propagate subset utility. `CountPaletteRowFreeAndReusableEntries`
at `004ed390` distinguishes free from preserved slots, and `AssignColorsToBestPaletteRow` at
`004ed4c0` chooses the row with greatest reusable-color overlap, then least required capacity,
inserts only missing colors into state-zero entries, and returns that row for the tile words.
`SelectPaletteColorSetsForCapacity` repeatedly chooses a capacity seed by reusable count and free
capacity, combines exact-fit records by existing-color overlap, direct occurrence weight, larger
sets, and earlier tile occurrence, then offers the complete proposal to every capable row.
`AssignColorsToBestPaletteRow` selects greatest overlap, then lower required capacity; row zero is
also its no-result sentinel, so an otherwise equal row-zero/row-one final choice resolves to row
one. `ExtendPaletteWithWeightedColorSets` then ranks remaining records by overlap and
aggregate utility but chooses individual inserted colors by direct weight.
`AssignImportedGraphicsToPaletteRows` finally records one row per 8×8 tile. The Rust multi-row model
must retain these stages; merely quantizing an entire image to one 15-color row is not equivalent.

The caller around `004f03d0..004f0784` also performs a distinct per-tile capacity reduction before
creating those records. `BuildTileUniqueColorHistogram` returns the raw sorted colors and counts.
If the count reaches the maximum free-row capacity and the chosen row's first reusable word is not
present, the target loses one slot; counts above capacity use the same rule, while a qualifying
first-entry match receives `$80` weight. `004ec7a0` and the four inline edge scans then increment
only colors whose current weight exceeds two for matching pixels directly above, below, left, and
right of the tile. `004ebe40` stably retains the strongest target colors, the caller maps all 64
pixels to their nearest retained RGB555 words, rebuilds the histogram, and only then calls
`FindOrCreatePaletteColorSetRecord`. A live 32×32/16-color trace proved capacity 12: the raw
12-color tile at `(8,16)` became 11 colors by removing `$5313`, and the raw 13-color tile at
`(16,16)` became 11 by removing `$264E/$4E8C`. Rust reproduces both the row-major mutation and
border-weight threshold; its exact-capacity regression locks the stable weak-color tie.

The global Median Cut request is likewise bounded by destination capacity rather than blindly
using the dialog's maximum. `ProcessBitmapGraphicsImport` counts writable entries, retries the
quantizer at `004eff17..004eff29` when the unmatched generated colors exceed that count, and
credits distinct preserved reusable colors toward the installable total. This is observable in a
32-color request with 21 free entries and eight identical reusable backdrop entries: the first
native quantizer call receives an effective ceiling of 22. None of those 22 generated colors
passes the preserved-backdrop substitution policy, so the original retry arithmetic lowers the
ceiling to 21 and invokes the quantizer again. Rust now reproduces both the initial
destination-derived ceiling and that substitution-aware retry. With that rule applied, the
retained 32-color fixture matches all 32 active palette words and every byte of the complete
`$000–$2FF` graphics workspace.

The Rust application path now carries that per-8×8 row plane through graphics materialization,
Map16 construction, and the converted preview. Each of a Map16 definition's four subtile words
receives the row selected for its own source 8×8 tile; no bitmap-wide or 16×16-wide row is inferred.
The final assignment follows the additionally confirmed `AssignImportedGraphicsToPaletteRows`
stage: it computes a nearest entry and weighted RGB555 error for every source color against every
usable row, sums those errors across each 8×8 occurrence, and chooses the least-error row before
rewriting its local indexes. This permits an uninstalled color to use its nearest available entry
instead of incorrectly rejecting every non-exact color set.
The record weight is pixel-semantic: identical sets accumulate each member color's 8×8 frequency,
and a superset adds the matching per-color frequencies of every strict subset before summing its
selection weight. The Rust records now retain direct and aggregate vectors separately; treating
all tile occurrences as unit weight would select the wrong colors for mixed-frequency artwork.

`ProcessBitmapGraphicsImport` at `004ef770` also fixes the reduction-method dispatch: method zero
calls `QuantizeRgbPixelsToSnesPalette`, while method one calls
`SelectOptimalColorsFromRgb555Histogram`. Priority therefore belongs to the Popularity path, not
Median Cut. The selector first requires a candidate's raw frequency to exceed the current weakest
slot. It then finds the nearest already selected or reusable destination color using the same
weighted RGB555 distance, raises that distance by repeated squaring for priority levels 2–4, and
adds `(distance × frequency) / $8EE09` with 32-bit wrapping arithmetic. The Rust palette-aware
Popularity entry point now reproduces that admission and priority core and the application passes
its actual destination palette.

The two optional adjacent-color passes are now recovered from `004ec2b1..004ec65c` and represented
independently. Their component starts use unsigned subtraction: a zero edge wraps and makes that
component scan empty rather than clamping it. Method 1 scans a 3×3×3 RGB555 neighborhood in
red/green/blue loop order. A stronger-or-equal selected neighbor marks the candidate consumed but
the scan continues; the first weaker neighbor is replaced, bubbled toward the front, and exits the
complete neighborhood scan. Method 2 uses the executable's asymmetric 5×4×3 window, exits without
aggregation at the first neighbor at least as strong as the candidate, otherwise selects the
weakest scanned neighbor, and combines only scores below `$80`. Its reorder comparison and
moved-entry score use the incoming score rather than the combined score, matching the assembly at
`004ec5f0..004ec655`. The Color Options dialog confirms
method 1 defaults enabled and method 2 defaults disabled; both switches are now exposed by the
native frontend and covered independently.
The native dialog exposes the recovered reduction method, 1–128 limit, 1–4 priority, and all 128
free/reusable/reserved entry states. Fixed palette ownership is forced reusable and ExAnimation
ownership forced reserved before allocation, so presentation choices cannot overwrite another
domain. Focused tests force the left and right quadrants into disjoint rows and verify both their
packed `$1c00` palette bits and row-aware RGBA preview. Exact priority-level influence during
high-color selection and Wine output equivalence remain oracle gates rather than assumed parity.

The final generated-color ordering is also semantic. `ProcessBitmapGraphicsImport` converts each
newly assigned SNES color through `ConvertRgbToHsl240` at `004ebc00`. Every run starts with the
lowest-lightness remaining color and greedily chooses the nearest subsequent color using the
integer metric `3·Δlightness² + 2·Δsaturation² + 8·Δhue²`. When both colors have saturation below
16, hue is ignored and the distance becomes `3·Δlightness² + Δsaturation²`. A hue discontinuity
above 45 starts a new run unless both colors are in that low-saturation band. The Rust allocator
now reproduces the 0–240 integer conversion, lowest-lightness anchors, run restart, and entry swaps
before mapping source pixels to row indexes; leaving colors in raw insertion order changes both
palette words and encoded graphics.

`MapRgbPixelsToReducedPalette` uses the same `4R² + 3G² + 2B²` RGB555 distance as final row
assignment. Expanded-RGB Euclidean quantization is not equivalent. The palette-row chooser scans
rows 0–7 but uses row zero as its no-result sentinel: an exact row-0/row-1 tie selects row 1, while
later exact ties retain the first nonzero winner. The bitmap oracle now captures the live 128-byte
entry-state map at `$009B3F58` before conversion because that effective map can differ from the
initializer after the user changes per-entry controls.

The earlier preserved-color substitution in `ProcessBitmapGraphicsImport` is now recovered as
well. When at least one palette entry is free, Lunar Magic repeatedly chooses the globally nearest
pair between an unmatched reduced color and an unused reusable palette color using its weighted
RGB555 metric. It accepts the replacement when the circular HSL240 hue difference is within
`DAT_005e5600`, subject to the recovered saturation/lightness guards; two low-saturation colors are
accepted regardless of hue, and a value of 240 disables all guards. A live Lunar Magic 3.63 process
reports the initialized value `$2D` (45). Rejected reduced colors are retired rather than tried
against a second preserved color. Rust now reproduces that ordering, one-use-by-color rule, default,
and exact 0–240 bound before global source-index mapping, and exposes the tolerance in the native
bitmap-import dialog.

The adjacent “Allow modifying palette colors that aren't marked on right” checkbox is the low bit
passed from `HandleConvertedBitmapPreviewWindow` and `HandleBitmapImportPreviewDialog` into
`ProcessBitmapGraphicsImport`. When clear, the quantizer and preserved-color substitution loop are
skipped. `CollectUniqueAvailablePaletteColors` instead copies every current palette word whose
state lacks reserved bit `$02`, then removes duplicates with the original's tail-replacement order;
the ordinary row allocator may still rearrange those existing colors into free slots. A normalized
live Wine capture with control `$74` clear, Maintain Detail clear, and unique priority set confirms
this route changes the palette arrangement and graphics while admitting no new reduced color. Rust
now exposes and implements that exact existing-colors-only policy and requires destination-palette
context when it is selected.

The exact fifth differential refines the allocator behavior for that route. A row may retain exact
matching free words during the first set-assignment pass, but those globally covered records are
revisited when extending later rows; matching words in the later row are ordinary movable
assignments rather than fixed retained entries. Final tile scoring prefers a row containing exact
movable assignments over an equally accurate row that merely retained those words. Within the
selected row, equal-distance duplicate words resolve to the later entry when control `$74` is
clear. These rules reproduce all
32 active palette words and every byte of the `$000–$2FF` planar graphics workspace.

Control `$65`, “Give priority to exact color matches in existing palette,” is checked and disabled
in Lunar Magic 3.63. Its byte `DAT_005e55cc` has exactly two references, both in
`HandleBitmapImportOptionsDialog`: initialization reads it and OK stores the disabled checkbox.
There is no processing-path reader. Rust therefore retains the checked preference as explicit state,
renders it disabled, and deliberately leaves conversion unchanged rather than inventing behavior.

The address-taken callback at `$004F1FA0` is now promoted and named
`HandleBitmapImportOtherOptionsDialog`. Its six flags map exactly to Optimize new 8×8 tiles
(`DAT_005e55f4`), reuse existing 8×8 tiles including all four flip orientations
(`DAT_005e55f5`), deduplicate 16×16 definitions/background paste (`DAT_005e55f6`), layer priority
(`DAT_00e27b31`), configured blank 8×8 tile routing (`DAT_005e55f7`), and configured blank 16×16
Map16 routing (`DAT_005e55f8`). The four hexadecimal values are first graphics tile
`DAT_005e55e0`, blank graphics tile `DAT_005e55ec`, first Map16 tile `DAT_005e55e4`, and reserved
blank Map16 tile `DAT_005e55f0`. A live dialog capture confirms defaults `$200`, `$0F8`, `$8200`,
and `$8000`, all optimization/blank switches enabled, and layer priority disabled.

Rust now carries all four values and six switches through one recomputed preview. An all-zero 8×8
source routes directly to the configured tile without changing graphics ownership or occupancy.
The blank-Map16 decision inspects the referenced decoded graphics pixels—not allocation occupancy—
so it remains correct when blank-8×8 routing is disabled. Deduplicated blank 16×16 blocks use the
configured reserved index without consuming a definition; sequential mode continues to materialize
them normally. Existing-tile reuse retains Lunar Magic's unconditional flip search rather than
exposing a native-only switch that does not exist in the original dialog.

The installed-session cross-product gate keeps these two blank switches independent. Across all
four combinations, transparent 16×16 source either references configured graphics `$0F8` without
allocating `$200` or materializes `$200`, and independently either maps to reserved Map16 `$8000`
without changing the workspace or consumes blank definition `$8200`. This also proves blank-Map16
classification remains based on decoded pixels when configured blank-8×8 routing is disabled.

The following controller and selection tooling through `004f5990` is now named. This includes the import-preview zoom menu and keyboard hook, the top-level bitmap import workflow, a textual remapping language that can transform graphics indexes, palette rows, Map16 indexes, and secondary-map values, and the custom registered `Lunar Magic 16x16 Tiles` clipboard serializer. Added the exact 0xA0-byte `LunarMagicTileClipboardHeader` with section offsets, selected count, rectangular dimensions, source Map16 index, flags, and explicitly represented reserved regions.

Map16 import/export, history, and visible rendering through `004f9e40` are now named and annotated. Added exact 64-byte `Lm16Map16FileHeader` and `Lm16Map16SectionDirectory` structures for the structured `.map16` format. Added the exact 811,788-byte `Map16UndoSnapshot` and typed its live linked-list globals. Rendering names now distinguish decoded tile composition, Acts Like overlays, selected-tile highlighting, page frames and labels, page boundaries, and bounded versus drag-selection marching ants.

`WriteAllMap16ContainerSections` at `004f78f0` and `ReadAllMap16ContainerSections` at `004f7c80`
confirm the semantic shape of a complete `.map16` container. The combined definition section is
exactly `0x80000` bytes: 65,536 definitions of four little-endian 16-bit subtile words. Its
foreground directory entry aliases the first `0x40000` bytes (tiles `$0000-$7fff`) and its
background entry aliases the second `0x40000` bytes (tiles `$8000-$ffff`). The separate
`0x10000`-byte behavior section is `0x8000` little-endian 16-bit Acts-Like values and therefore
belongs only to the foreground namespace. Background definitions have no Acts-Like value.
Wine cross-oracles accept the Rust canonical container with the three editor-only sections absent,
and re-export its semantic core byte-for-byte. In the reverse direction, replacing the core of a
real 651,760-byte template preserves every auxiliary and editor-state byte exactly.

The live editor buffers use the same contiguous split: primary definitions begin at `00777e58` and
secondary/background definitions begin at `007b7e58`, exactly `0x40000` bytes later.
Once the expanded foreground runtime is installed, only the first `0x1000` bytes of the legacy
transferred definition table remain authoritative. A zero block-zero pointer blank-fills the rest
of that block; it must not expose the legacy table's tile `$0200+` tail.
`SaveSecondaryMap16DataBlocks` divides the background half into eight `0x8000`-byte blocks. It
trims trailing `0x1004` words, rounds each retained length to eight bytes, keeps block zero at the
legacy fixed location when it fits within `0x1000` bytes, and otherwise allocates a relocatable
payload and writes one packed three-byte pointer into the descriptor's eight-entry secondary
pointer table. This directly explains why bitmap import's default cursor `$8200` addresses the
background half rather than an extension of the foreground Acts-Like table.

An independent command-line Wine oracle imported a complete container whose only semantic change
was tile `$8200 = {1111,2222,3333,4444}`. Re-export reproduced those four words exactly. Lunar
Magic stored secondary block zero in a `STAR` payload at logical `$80008` with length `$1008`:
the `$200`-tile displacement contributes `$1000` bytes and the changed definition contributes the
final eight bytes. The first pointer-table entry at logical `$77d50` was `$108008` with its high
bank bit mirrored away in storage, and the installation marker at logical `$28da4` became `$22`.
This dynamically validates the recovered trimming, eight-byte rounding, and low-bank pointer
interpretation.

The runtime installer is materially larger than the eight-pointer table. Ghidra shows
`SavePrimaryMap16DataBlocks` calling `AllocateAndInstallMap16AuxiliaryTables` at `00470060` and
`FinalizeMap16RuntimeInstallation` at `00470490` when the initialization probe is erased.
`InstallPrimaryMap16PointerRuntime` at `0046f120` writes six hook families, rewrites nine primary
pointer pairs, normalizes legacy hooks, and applies mapper-specific relocations. The auxiliary
installer allocates one relocatable code/table block, derives four internal table pointers, writes
numerous fixed hooks, and patches three IRAM operands. Consequently the Rust secondary saver only
publishes data when this runtime is already authenticated; treating marker `$22` alone as an
installer would produce a ROM that reopens in the editor but cannot execute correctly in-game.

A second Wine installation started from a 1 MiB ROM whose entire expansion area was deliberately
occupied with `$A5`. Lunar Magic expanded it to 2 MiB, moving secondary payload `$80008` to
`$100008` and the auxiliary payload `$88000` to `$108000`. Comparing the two installed ROMs
changed only the expected allocation-bank operands (plus ROM size and checksum); the complete
auxiliary payload was byte-identical. An unchanged complete-container import then isolated the
runtime itself from secondary user data: its first secondary pointer is fixed storage `$0D9100`,
the marker is already `$22`, and its sole RATS block is the auxiliary `$8000`-byte payload.
The Rust installer now accepts both authenticated source shapes, scans real and virtual fill bytes
for the first bank-aligned auxiliary payload, expands an occupied 1 MiB source to 2 MiB, places its
RATS header at `$107FF8` and payload at `$108000`, and preserves either copier-header form through
checksum-valid reopen and byte-exact undo.

The authenticated Rust installer models that result as 302 exact-precondition fixed writes, one
typed low-bank relocation into an independently allocated auxiliary payload, and regenerated ROM
size/checksum data. Its CLI output from the same copier-headered pristine ROM is byte-for-byte
identical to Lunar Magic 3.63's 1,049,088-byte control output. Native bitmap import now invokes this
installer before committing a pristine background tile such as `$8200`; the resulting secondary
block reopens at the Wine-authenticated trimmed length `$1008`, while the complete multi-domain
operation remains one application undo.

The adjacent named compatibility stages are independently versioned. A retained Lunar Magic 3.01
ROM contains the complete `$037600` 68-byte hook base, marker `LM $0111` at `$03765C`, and the old
20-byte destination at `$0377A0`. Importing an unchanged complete Map16 file into that ROM with
Lunar Magic 3.63 changes only the marker to `LM $0112` and replaces the destination with
`20 08 F6 A5 0F F0 04 A3 0A 80 02 A3 06 C9 DA F0 0F 4C 02 F6`; its other changes are the editor
version stamp. This agrees exactly with `InstallMap16HookStageFour` at `0046FFB0`, which first
requires `CheckLegacyMap16HookStageThree`, writes the 20-byte blob from executable address
`005B77CC`, writes the four-byte marker from `005B77F0`, and patches byte `base+$1AE` from descriptor
entry `$3E4`. The Rust migration authenticates all three staged regions, retains every live Map16
table/allocation, applies those two effective changes transactionally, and matches the Wine oracle
outside the editor stamp and explicit checksum policy.

The same runtime exposes eight foreground definition blocks, not one monolithic allocation.
`SavePrimaryMap16DataBlocks` scans eight consecutive `$8000`-byte slices of the live primary
buffer. Block zero retains its first `$1000` bytes in legacy fixed storage and allocates only the
trimmed suffix; blocks one through seven allocate their complete trimmed prefix. Every retained
length is rounded to eight bytes. A Wine matrix changing only tiles `$0800`, `$1000`, `$2000`,
`$4000`, and `$7fff` produced primary payload lengths `$3008`, `$0008`, `$0008`, `$0008`, and
`$8000`, exactly matching those slice rules. Touching block four or later also publishes the
second `$8000`-byte auxiliary table described by the same save routine.

The installed displaced-low-word/bank-byte operands begin at logical runtime base `$37540`. Their eight
offset pairs are `{13,17}`, `{1c,20}`, `{27,2b}`, `{30,34}`, `{54,58}`, `{5d,61}`, `{68,6c}`,
and `{71,75}`; the corresponding pointer displacements are `$1000,$8000,$0001,$8001,$0000,
$8000,$0001,$8001`. The first changed `$0800` oracle, for example, stores pre-displacement
pointer `$107008`, which resolves to allocated payload `$108008` after adding `$1000`.
The second location in each pair is the bank byte itself, not the beginning of a writable
little-endian word: the preceding byte in the installed 65C816 immediate operand remains fixed
zero. `ReadPackedRomWordForTableEntry`, `RewritePrimaryMap16PointerPair`, and the Wine ROM deltas
agree; the `$0800` case changes only logical `$37557` from `$00` to `$10`.

Complete foreground behavior is a separate `$10000`-byte concern. `LoadPrimaryMap16DataBlocks`
initializes entries `$0000-$01ff` to their own tile numbers and every remaining entry to `$0130`.
The installed runtime stores entries `$0000-$3fff` in its first raw `$8000`-byte auxiliary block
(direct pointer operand at logical `$37624`) and entries `$4000-$7fff` in its optional second raw
block (displaced pointer operand at `$3763a`). A Wine import changing only tile `$1000` Acts-Like
to `$0123` changed exactly the corresponding word in the first auxiliary payload plus checksum; a
tile `$4000` definition/Acts-Like import placed `$0123` at word zero of the second payload.

This is distinct from `CommitExpandedMap16ActsLikeTable`, whose compressed split streams have a
`$4000`-entry maximum and use `$fc7a` as their trim sentinel. The complete `ImportAllMap16`
oracles did not alter those compressed streams. Consequently complete foreground `.map16`
persistence owns the two raw auxiliary halves and must not pad or rewrite the transferred helper.

Map16 interaction code through `004fd510` is now named and annotated. This covers tracking tooltips (including disabled controls and internal sprite/background tile descriptions), independent 100-5000 percent zoom with forward and inverse coordinate transforms, DPI-aware selector sizing, hover and auto-scroll behavior, selection creation and movement, temporary-buffer restoration, property-panel mixed-value analysis, priority and palette edits, horizontal and vertical flips, and Acts Like cycle detection.

Map16 Acts Like editing and the full 8x8-subtile interaction path through `004ff170` are now named. The recovered behavior includes cycle-rejecting Acts Like assignment, per-corner graphics-index edits, additive/subtractive 324,608-byte selection masks, clamped selection translation, live/temporary buffer swapping, 8x8-selector paste integration, hover and auto-scroll logic, and serialization/deserialization. Added the exact 0xA0-byte `LunarMagicSubtileClipboardHeader` with tile and auxiliary section offsets, selection dimensions, count, source index, flags, and reserved regions.

Native `Lunar Magic Map8 Tiles` copy/paste entry points and fallbacks are now named through `00500850`, including conversion from graphics-selector indexes, legacy BG Tiles v3 handling, raw subtile rectangle placement, and extraction/publishing helpers. The Map16 render child procedure is also identified in full, together with its key-command table dispatcher, character shortcuts, 256x256 32-bit DIB allocation, `.m16` fixed sidecar output, and block-rounded `.s16` sprite sidecar output.

The Map16 modeless parent dialog is now fully named through `00503ae0`, including its command dispatcher, exhaustive control initialization, DPI resize paths, render-child creation, teardown, and show/close entry points. The adjacent graphics editor is named through `00504fe0`: status-bar lifecycle, sixteen-by-sixteen color-map filter editing, indexed-to-SNES-4bpp encoding, active palette selection, editable 8x8 pixel grid, foreground/background swatches, complete tile-sheet rendering, commit propagation, zoomed presentation, and refresh paths.

Rust routes selected-tile horizontal and vertical flips through each graphics surface's selected
edit buffer. The portable document retains its revisioned document behavior; pristine-layout and
profile-backed ROM editors now match Lunar Magic's separate private-buffer boundary, so transforms,
color mapping, and painting do not modify backing until a permitted sheet paste. The three
tile sheets now give unmodified Up/Down their authenticated page role: Up moves to the same offset
on the previous 256-tile page and Down moves to the next, with bounded partial-page handling, focus
transfer, and automatic scrolling; Left/Right only enter the Shift-modified pixel-wrap path.
Authenticated Page Up/Down input steps forward/backward without cycling from the default palette
through each surface's available display-palette rows. Their shared transient hover outline
accompanies an event-driven status line with recovered tile/address, color, selection, palette,
page, and boundary messages.
Their selected-tile pixel grids share one geometry source for rendering and hit testing. A direct
reference audit corrects the earlier interpretation of `005E54CC`/`005E54D4`: these hold horizontal
and vertical window-DPI percentages, initialized from the window at `0050693D`–`00506955`, not a
user-selected tile zoom. `LayoutGraphicsEditorForDpi` at `005063C0` scales the base control geometry,
and `005066BA`–`005066F0` materializes an exact `0x100`-pixel logical selected-tile canvas. Rust now
uses that fixed 256×256 logical square and delegates DPI scaling to the toolkit, removing the
non-native discrete selector. The documented 8×8-editor Shift+Arrow behavior is implemented as a
one-pixel directional wrap. Direct decompilation also proves asymmetric modifier tests: Left/Right
wrap whenever Shift is held; Up/Down wrap only when Shift is held without Ctrl or Alt, otherwise
those vertical keys retain page navigation (including modified forms and Shift on a read-only tile).
Page Up advances the palette with any modifiers, while Page Down reverses it unless Ctrl+Shift
invokes the separate internal-cache unlock. Exact
Ctrl+left-click copies and selects the pointed tile; adding Shift or Alt instead follows ordinary
selection. Right-click tests only the Ctrl state: without Ctrl it copies the active edit tile over
its target, while Ctrl with any other modifiers requests the typed single-tile clipboard payload
and selects that target only after validation, matching the recovered mouse-message branches.
All four tile-sheet routes dispatch directly from their primary/secondary button-down branch rather
than waiting for a completed click.
Installed paste cannot bypass fixed/ExAnimation ownership, stale revision,
or active file-worker gates.
`HandleGraphicsEditorWindowMessage` passes the `WM_KEYDOWN` virtual-key value directly to
`ProcessGraphicsEditorKeyboardInput`; this corrects an earlier off-by-one interpretation of the
function-key cases. Unmodified F9 is not a ROM-commit shortcut. It enters the current-level GFX
publication path described below. The Rust ROM editors retain explicit commit buttons for their
extension workflow, while only the portable standalone document keeps its own F9 save binding.
Direct decompilation of `ApplyColorMapFilterToGraphicsTile` at `00503ce0`,
`DrawColorMapFilterPreview` at `00503db0`, and `HandleColorMapFilterDialog` at `005040e0` proves
sixteen filters of sixteen 4-bit destinations. Application maps every decoded tile pixel through
the selected row before 4bpp re-encoding. The dialog previews the base and mapped rows, edits one
source/destination pair, resets the selected row to identity, snapshots all 256 entries on open,
and restores them on Cancel. Rust now models those exact dimensions and transactional semantics
and routes selected-tile application through each graphics surface's existing mutation boundary.
`HandleGraphicsEditorCharacterShortcut` at `005061e0` indexes a 54-byte dispatch table for both
uppercase and lowercase characters from D through Y. Its only non-default entries are D, M, R, X,
and Y, which synthesize the existing Do Map, Map Colors, Rotate 90°, Flip X, and Flip Y button
commands respectively. `HandleGraphicsEditorWindowMessage` calls it from `WM_CHAR`, so ordinary
lowercase input and Shift-produced uppercase input both dispatch, while Ctrl character controls and
Alt system characters do not enter that branch. The Rust editors preserve that translation; rotation
is a first-class clockwise indexed-tile transform and all five routes retain each surface's normal
controller, ownership, stale-revision, and worker guards.
`HandleGraphicsEditorCommand` at `005054d0` confirms that Rotate 90° is also a directly reachable
control command, alongside the two flip controls, rather than a keyboard-only operation. The three
Rust graphics surfaces now expose the missing visible rotation control and route all three buttons
and their R/X/Y shortcuts through one shared transform action while preserving their existing
mutation gates.
The same dispatcher maps four additional controls to synthesized `VK_UP`, `VK_DOWN`, `VK_PRIOR`,
and `VK_NEXT` messages before restoring focus to the tile sheet. They are the visible previous/next
page and previous/next palette routes for the already recovered keyboard branches. Rust now exposes
all four controls on every graphics surface; control and keyboard actions share the same bounded
selection/palette transitions and exact page, boundary, and rendered-palette status messages.
Ctrl+Shift+Page Down is a separate diagnostic branch: it sends command `$24E7`, raises maximum page
global `005E54F0` from its initialized `$05` to at least `$3F`, and reports `Internal GFX data
viewing unlocked.` Window creation sends the same command when internal option byte `00E278D0` is
set. `RenderGraphicsTileIntoEditorSheet` addresses the decoded cache from `$006204B0` as 64 bytes
per tile. The recovered population and viewer-label calls divide its `$4000` tiles into current
FG/BG `$000-$3FF`, current sprites `$400-$5FF`, GFX33 `$600-$77F`, selected auxiliary animation
`$780-$87F`, GFX32 `$900-$BE7`, four `$400`-tile ExAnimation banks `$C00-$1BFF`, eight `$80`-tile
Layer 3 banks `$1C00-$1FFF`, and eight `$400`-tile `ExSpriteGFX00-07` banks `$2000-$3FFF`.
`LoadAnimationAndPlayerGraphicsCaches` proves GFX33 and GFX32 destinations and exact `$180`/`$2E8`
extents. Its four source offsets `$18000/$20000/$28000/$30000` and four `$8000`-byte sizes select
reserved graphics files GFX60–GFX63 and decode exactly `$400` tiles into each ExAnimation bank;
they are not relative copies chosen by the active level/global animation setting.
`LoadExternalSpriteGraphicsAndPalette`
uses the parallel eight-entry table for the final banks. The pristine Rust editor materializes the
complete cache with exact ROM-owned banks, zeroes absent auxiliary/ExAnimation/external-file banks,
retains the original locked default, and consumes Ctrl+Shift+Page Down only from the focused tile
sheet to expose pages through `$3F` with the original status. It refreshes on active-level and
Special World changes while retaining ordinary staged file edits, accepts transient cache edits,
applies the original `$000–$5FF` sheet-paste boundary, and routes its exact current-level slots to
F9. The profile-backed installed editor
additionally resolves the current level's legacy or six-slot bypass files, installed GFX32/GFX33
and Layer 3 sources, loads reserved source files GFX60–GFX63 into the four exact ExAnimation banks,
selects auxiliary bank `$780-$87F` from expanded-header field 0 when bit `$8000` is set or from the
last legacy object-control command `$25/$26` parameter minus one, and boundedly reads all present
ROM-sibling `ExternalGraphics/ExSpriteGFX00–07.bin` files into their eight exact bases. The
diagnostic cache stages pixel and transform mutations in the private edit tile. Original
right-click sheet paste rejects tiles above `$5FF`, unused `$300–$3FF` when bypass is off,
fixed-animation `$41–$81/$90–$91/$DA–$DD/$EA–$ED` when vanilla animation is enabled, and
Special World SP2 `$480–$4FF`. The isolated Wine `graphics-cache-paste` oracle proves successful
source-identical mutation at `$002/$5FF` and unchanged rejection targets `$041/$300/$600`; F9
persists only the current-level
FG/BG/SP working buffers described below, leaving edits to all other diagnostic banks transient.
The installed editor applies that same predicate to both clipboard and selected-buffer paste entry
routes. Toolkit paste events and the native extension button can arrive without the sheet gesture
that requested them, so neither may bypass the `$5FF`, bypass, animation, or Special World guards.
The same window procedure and `ProcessGraphicsEditorKeyboardInput` at `005059f0` establish the
status lifecycle. Mouse movement publishes `Tile 0x%X (Address 0x%X)`, `Color %X.`, or the active
tile-edit selection. `HandleGraphicsEditorWindowMessage` at `005068c0` proves that primary and
secondary palette clicks select foreground and background colors, while primary and secondary
pixel-grid gestures paint with those colors. Both palette selections happen directly in the
respective button-down message branches and do not inspect keyboard modifiers. Ctrl changes the
pixel-grid actions into foreground
and background sampling. The corresponding messages are exactly `Color %X selected for FG.` and
`Color %X selected for BG.`; initialized globals `005E54F4` and `00E27B84` establish defaults 1 and
0. The button-down branches choose paint or sample once: paint stores the selected color in
`00E27B88`, sets capture byte `00E27B32`, and subsequent mouse moves reuse that mode without
consulting Ctrl; sampling does not set capture. Rust retains the same per-press mode across modifier
changes. Successful Up/Down
navigation publishes `Viewing 8x8 page 0x%X.`, while blocked movement publishes the exact
`Already at Start`/`Already at End` diagnostics. Page Up/Down report the rendered hexadecimal
palette. Their native branches inspect modifiers only for the edit-vs-navigation and diagnostic
exceptions above; they are not globally unmodified-only shortcuts. Rust now keeps this state
event-driven so a stationary pointer does not overwrite a later
keyboard action and leaving the tracked region clears it, matching the Win32 message boundary.
Movement within the same tile still re-enters the native mouse-move branch and recomputes its
Ctrl+Shift animation attribution; only a stationary pointer suppresses that hover refresh.
Palette swatches and the selected-tile pixel canvas use the same rule: another mouse-move message
inside the current region republishes `Color %X.` or the active tile selection, while modifier or
repaint frames without movement leave the most recent action diagnostic intact.
The Rust event adapter additionally preserves the single-window message ordering across its three
sequential toolkit regions: inactive regions do not clear a status just emitted by the active one.
The F1 table entry at `00505A1E` does not invoke help: it posts command `$1B59` to the main
level-editor child. `HandleLevelEditorCommand` fans that command out as redraw command `7000` to
the graphics, palette, Map16, background, ExAnimation, and other dependent editor windows, then
rerenders the shared 512-pixel surface. The branch performs no modifier checks, model mutation, or
status update. Rust therefore consumes every modifier form of F1 on each graphics surface and
requests a toolkit repaint without fabricating a reload or message.
The same keyboard jump table maps virtual key `$77` (F8) to the branch at `00505A4D`. Ordinary F8
toggles byte `00E27B90`; Ctrl+Alt+F8 instead toggles grid DWORD `005E54F8` between its initialized white
`$00FFFFFF` and black `$00000000`, reporting `Tile grid color 1.` or `Tile grid color 2.`. The
renderer at `00504D00` overwrites every sixteenth row and column of the 256×256 page DIB, proving a
seamless 16×16 array of 16-pixel tile cells rather than spaced widgets. The selected page global
`00E27B80` determines which 256-tile slice is rendered.
The adjacent virtual-key `$78` (F9) branch at `00505B7B` displays
`Save level GFX to Graphics folder?` and calls
`00480B60` after confirmation. That routine walks six FG/BG source buffers at `0086B7E8` and four
SP buffers at `008737E8`, each with a `$1000`-byte stride, pairing them with the active file-number
tables at `008F3918` and `0061FC38`. It therefore exports the current level selection rather than
the complete standard GFX table, and its buffers prove that even native 2bpp/3bpp sources are saved
as decoded 4bpp files. The sprite loop conditionally omits its second slot when `00E278DF` is set;
the ordinary path visits all four. Cross-references prove that `00E278DF` is the non-persistent
`Special World Passed Graphics` view flag: `LoadSpecialWorldGraphicsFile` (`00464890`) decodes the
half-size 3bpp GFX31, synthesizes the absent fourth plane, and installs it in the SP2 working slot.
F9 omits the ordinary SP2 filename because that working buffer no longer represents it. Before
opening any output, the separate-file path verifies that the full standard set `GFX00.bin` through
`GFX33.bin` exists in the ROM-sibling `Graphics` directory and contains no directory entries. Each
selected output is then opened in truncating write mode. Its exact
completion messages are `Saved FG/BG/SP GFX to files.` and `Couldn't save FG/BG/SP GFX to file!`.
Rust now exposes this F9 workflow on both pristine SMW-US and profile-backed installed ROM
graphics editors. The pristine path reads the authenticated vanilla object/sprite assignment tables
for the globally active level, while the installed path additionally honors expanded Super GFX
bypass records. Both require the complete existing standard file set before staging and publish the
selected decoded `$1000`-byte replacements as one recoverable group, which is stronger than the
original per-file truncation on a mid-publication failure. They substitute the active staged slot
when it belongs to the exported set. Once the diagnostic cache is unlocked, F9 instead encodes the
exact six FG/BG and four sprite cache slots. Legacy levels retain `$7F` in unused FG/BG slots,
Special World substitutes `$7F` for SP2, and repeated file IDs use the last visited working buffer,
matching the original sequential writer. A separate visible Rust extraction button retains the useful
create-new-directory workflow without claiming the original shortcut. Standard selections resolve
under the ROM-sibling `Graphics` directory, while selectors `$34+` other than the ignored `$7F`
resolve to canonical two- or three-digit `ExGFX` names under sibling `ExGraphics`; every selected
extended destination joins the same preflight and recoverable publication group. Rust's View menu exposes the
same ephemeral Special World option; pristine and installed previews substitute decoded GFX31 into SP2, and F9
omits the normal SP2 assignment before stable duplicate collapse just like the native loop.
When persisted byte `00E278C0` is enabled, the same writer uses `Graphics/AllGFX.bin` instead of the
52 separate standard files. `HandleLevelEditorCommand` command `$24BD` toggles that byte, and
`SynchronizeApplicationSettingsRegistry` loads/saves it. `CalculateAllGfxFileOffset` sums the exact
52-entry DWORD table at `005E8100`; entries `$00..$26` are `$1000`, `$27` is `$0C00`, `$28..$2B`
are `$0800`, `$2C..$2E` are `$1000`, `$2F` is `$0400`, `$30..$31` are `$0800`, `$32` is `$5D00`,
and `$33` is `$3000`, for an aggregate `$36D00`. `WriteExtractedGraphicsFileByIndex` opens the
existing joined file for update, seeks to the summed offset, replaces only the selected range using
that entry's native length, and continues to route selectors `$34+` to `ExGraphics`; `$7F` remains
an ignored internal sentinel. Rust exposes and persists the same joined/separate choice, validates
the exact existing joined shape plus every selected extended destination, patches the selected
ranges, and publishes AllGFX plus ExGFX replacements as one recoverable group.

The top-level transfer call matrix is now recovered as well. `HandleLevelEditorCommand` calls
`ExtractAllGFXFiles` with bit 0 copied from the persisted joined-file option; its quick/quiet form
also sets bit 1. `InsertAllGFXFiles` receives the same joined-file bit, with bit 1 suppressing the
completion dialog, bit 2 suppressing the ordinary open/save wrapper, and bit 3 selecting the
alternate progress target. `ImportExtendedGraphicsIntoRom` uses the corresponding quiet, wrapper,
and progress bits but has no joined-file bit. The ordinary menu commands first run their separate
confirmation resources; quick commands set bit 1 and bypass only the completion presentation, not
validation or mutation behavior.

Both bulk extraction functions use the same recovered `wb` mode string at image address
`005B2D3C`. `ExtractAllGFXFiles` therefore truncates/replaces every fixed
`Graphics/GFX%02X.bin` output, or `Graphics/AllGFX.bin` in joined mode, on a repeated export;
`ExportExtendedGraphicsFromRom` does the same for populated `ExGraphics/ExGFX%02X.bin` and
`ExGFX%03X.bin` outputs. The retained
`graphics-extraction-publication/oracle.tsv` binds the two function addresses, mode pointer and
bytes, names, directories, and flag meanings. Rust matches the visible replacement semantics while
publishing the entire separate-file set atomically, preserving regular-file permissions, and
rejecting symlink or non-file destinations without exposing a partial refresh. Joined and raw Save
As extraction use the same recoverable replacement boundary.

Regular-GFX insertion has one format-transition warning before any graphics structures are erased.
On a modified ROM with the ExGFX/ExAnimation support state present but without the expanded graphics
format marker, Lunar Magic displays `Graphics Format Change Warning!` and explains that existing
3bpp ExGFX will appear garbled until ExGFX is reinserted as 4bpp, ending with `Proceed anyway?`.
Choosing No returns before `InstallBulkGraphicsSystemPatches`. The Rust profile boundary now exposes
`requires_smw_us_v1_4bpp_graphics_warning`: it requires an authenticated native ExGFX runtime and
the absence of the complete two-byte-pair 4bpp marker, so ROM size, one coincidental `$32`, or a
foreign hook cannot trigger the migration route. `InstallBulkGraphicsSystemPatches` at `$0045C400`
installs its shared relocation/runtime family before the 52 regular-GFX slots are removed and
reallocated. Rust mirrors that ownership order for pristine installs and authenticates the installed
family for the affirmative route. The migration preserves ROM size plus all reserved, ordinary, and
extended ExGFX pointer bytes, restores the two 4bpp markers, checksum-repairs, and reopens all 52
regular files and a retained `ExGFX80` payload. `SelectGraphicsPointerTableFormat` at `$0045C030`
proves it selects the expanded table only when its descriptor-routed marker is `$EA`; live
differential writes resolve the complete marker to logical `$002A47 = EA EA`. Protecting the
`$088000..$08ACFF` extended table and zero-initializing both compressed pointer domains prevents
standard payload allocation and `$FF` sentinels from masquerading as ExGFX entries. A live Lunar
Magic 3.63 control performs original `-ImportGFX`, `-ImportExGFX`, and `-ExportExGFX`; the same gate
then re-exports every Rust-migrated regular file and the Rust-created `ExGFX80` byte-for-byte.
Metadata copying is neither used nor required for recognition.
The same import trace calls `InstallExpandedLevelSettingsPrerequisites` at `$00462C20`, which in
turn installs the ExAnimation control family and the two shared-palette hooks before publishing the
expanded level-header owner. Rust now invokes the already recovered current ExAnimation installer
when that independently authenticated generation is absent. The live gate byte-matches Lunar
Magic's fixed `$0026B8` and `$02D8E2` hooks plus `$077550..$07756F` helpers, then requires the
complete runtime detector to report `Current`. That detector permits populated reserved ExGFX
entries only when their three-byte addresses resolve to real bounded RATS payloads.
The retained command-line oracle now crosses this complete transition with present and absent
copier headers. Both Rust results have identical logical bytes; the headered path retains the exact
input prefix, the headerless path remains headerless until Lunar Magic opens its copy, and both are
recognized by Lunar Magic's GFX and ExGFX exporters.
Repeating that pipeline with map mode `$30` proves Fast LoROM shares the ordinary LoROM graphics
addressing path: the final identity remains Fast LoROM and both original exporters accept it.
The native insertion transaction now follows that prerequisite relationship for older installed
ROMs as well: authenticated 1.70-era pointer hooks and 1.65-era global-table records are migrated to
the current ExAnimation generation in staging before any requested ExGFX pointer is published.
A retained Lunar Magic 3.63 Wine oracle opens the original `Window8x8` through command `$232A` and
posts F9 directly to that window. With all separate files replaced by equal-length sentinels, level
`$105` changes exactly `$00,$01,$13,$14,$15,$17,$1B,$20`; removing `GFX33.bin` first produces
`Couldn't open file!` and changes no file. After `$24BD` enables joined mode, an independently
constructed expected `AllGFX.bin` that restores only those eight table ranges is byte-identical to
the observed output. The executable, pristine input, original-expanded ROM, manifests, ranges, and
hashes are retained under `docs/oracle-work/lm363/pristine-us/level-gfx-f9/` and bound by
`retained_lunar_magic_f9_oracle_binds_separate_and_joined_publication`.
With Ctrl+Shift held, `HandleGraphicsEditorWindowMessage` indexes the tile-attribution byte at
`006136B8`; its two independent key-state tests do not reject Alt. Zero has no animation
attribution, `$01-$7F` encode OrigAnim slot minus one,
`$80-$BF` encode a level ExAnimation slot, and `$C0-$FF` encode a global ExAnimation slot. Canonical
`LMGFXOWN` version 2 preserves those three bounded classes directly, allowing the installed Rust
editor to emit the exact recovered hover messages; version-1 generic record evidence still decodes
without pretending that it identifies one of those classes.
`LoadGraphicsEditorPaletteColors` (`00504860`) proves that the negative palette index is also a real
rendering state: it copies sixteen RGBQUAD entries from `005E7B60`, whereas nonnegative indices copy
the active palette bank. Rust models that default selection explicitly and reproduces all sixteen
recovered RGB colors across the picker, tile sheet, color-map dialog, and selected-tile view.

## Compression formats

The primary codec is the SMW/Lunar Magic LZ2 command stream. It supports literal copy, repeated-byte fill, repeated-word fill, incrementing-byte fill, and dictionary/back-reference operations. Commands use compact or extended headers and terminate with `0xFF`.

Stream mode 2 is byte-run RLE. Controls `0x00`-`0x7f` copy `control + 1` literal bytes. Controls `0x80`-`0xff` repeat the following byte `(control & 0x7f) + 1` times. `0xff 0xff` terminates a nonempty stream. Stream mode 3 uses the same block representation but is bounded by the expected decompressed length.

## Important typed globals

- `g_dwLoadedRomSize` (`00777e50`)
- `g_hLoadedRomFile` (`005e7354`)
- `g_dwCurrentLevelNumber` (`005e7738`)
- `g_dwSaveTargetLevelNumber` (`00853a44`)
- `g_dwLayer1ObjectDataSize` (`0091c550`)
- `g_dwLayer2ObjectDataSize` (`00857bb8`)
- `g_dwLayer1DataPcOffset` (`0091ce9c`)
- `g_dwLayer2DataPcOffset` (`00770e24`)
- `g_dwRomCompressionMode` (`009203b4`; currently `uint`, pending enum application)
- `g_dwRomStreamOffset` (`00921598`)
- `g_dwMemoryRomSize` (`009215ac`)
- `g_pMemoryRomImage` (`00e27978`)
- `g_pActiveRomStream` (`0092159c`; `(FILE *)1` selects the Lunar Magic ROM backend)
- `g_bRomHasCopierHeader` (`00608f53`)
- `g_pActiveRomLayoutDescriptor` (`009203c8`)

### Native level pointer tables

Dynamic comparison of Lunar Magic 3.63 saves confirms that layer-1 pointers are 512 contiguous
24-bit entries at logical ROM offset `0x2E000`, and layer-2 pointers are the adjacent table at
`0x2E600`. Sprite pointers are split: 512 little-endian low words begin at `0x2EC00` with a
two-byte stride, while the bank byte is supplied separately. `LoadSpriteDataPcOffsetTable`
(`004810e0`) proves both supported forms: descriptor index 24 supplies one shared bank, or, when
the hook marker at descriptor index 50 is `$22`, descriptor index 51 supplies a parallel bank-byte
table. Lunar Magic 3.63's installed SMW-US descriptor contains headered offsets `0x02EE00`,
`0x02DAF5`, and `0x077300`; pristine SMW uses logical offset `0x02EC00` for the low words and
the `$07` shared-bank operand at logical `0x02D8F6`. The `0x077100` parallel bank table belongs
to Lunar Magic's expanded representation and is erased in the pristine ROM. The level-000 and level-105
save cases independently changed bank entries 0 and `0x105` to `$10`, exactly matching MWL source
addresses `$108549` and `$108640`. The Rust layout model therefore represents contiguous,
split/shared-bank, and split/per-entry-bank encodings explicitly, including atomic writes and
profile-wide protection/auditing of every component.

The installed form is also verified as a Rust-to-Lunar-Magic write boundary. On an authentic
2 MiB modified ROM, the native level editor moves one sprite in level `$102` to the next screen,
stably sorts all legacy records by screen, and inserts another sprite through the canvas. The
grown payload relocates: `$102`'s split pointer changes to the newly allocated stream, `$101`'s
resolved pointer remains exact, and `$102`'s Layer 1 pointer and payload remain byte-exact. This
last invariant comes from splitting nonshared controller commits by changed domain rather than
running the aggregate two-stream allocator for a sprite-only edit. Checksum repair passes and Rust
reopens the complete result.
Lunar Magic 3.63 exports the result with the same Layer 1 aggregate as its export of the untouched
baseline and with the exact same grown/sorted sprite aggregate as Rust. Comparing two Lunar Magic
exports is required here because 3.63 independently canonicalizes two legacy compact screen-exit
records in the source ROM even when no Rust edit is present. Native undo restores every logical
input byte.

An MWL import/export control isolates Layer 1 ordering from its extent field. Importing objects
whose highest visible screen was `$00`, `$12`, `$13`, `$14`, or `$1F` produced stored last-screen
values `$12`, `$12`, `$13`, `$14`, and `$1F` respectively because the unchanged sprite stream
still reached `$12`. Thus import writes the maximum visible object-or-sprite screen exactly.
However, an injected `$1F` object followed by a jump back to `$00` was re-exported in that same raw
order. Unlike sprites, Layer 1 objects are not globally sorted merely by loading or serializing;
only positional edit operations own transition regeneration and sorting.

Layer 1 growth is now reciprocal on the same authentic installed ROM. Lunar Magic's expansion is
predominantly zero-filled after the installed RATS owners, so the expanded-ROM allocator accepts
both `$00` and `$FF` runs while continuing to exclude every discovered owner and protected range;
the pristine path remains `$FF`-only. A native canvas insertion relocates only level `$102`'s
Layer 1 pointer, retains its sprite pointer and level `$101`'s Layer 1 pointer, reopens in Rust,
and exports through Lunar Magic as exactly the baseline-canonicalized stream plus the inserted
ordinary object. Checksum and exact undo gates pass.

### Pristine graphics pointer planes

`ReadGraphicsFileRomPointer` (`00463A90`) proves that pristine GFX files
`$00..$31` do not use the contiguous 24-bit pointer table assumed by the
expanded profile backend. Descriptor entry `$2A` (`+$A8`) supplies headered
base `$003B92`, hence logical base `$003992`. Lunar Magic reads the pointer's
low, high, and bank bytes from three parallel 50-byte planes at logical
`$003992`, `$0039C4`, and `$0039F6`. Entries `$32` and `$33` use the separate
packed-pointer operands at descriptor entries `$2C`, `$2D`, and `$2B`.
Expanded GFX/ExGFX ranges use still other descriptor-selected tables.

The three special-file descriptor entries resolve to live startup-code operands rather than the
unchanged six-byte data pair at logical `$003882`: GFX33's low word is at `$00388B`, GFX32's low
word is at `$0038D8`, and both share the bank byte at `$003890`. An authentic 2 MiB Lunar
Magic-created ROM retains `$08BFC0/$088000` at `$003882` while changing the live operands to
`$088000/$089C68`; decoding the stale GFX33 address fails immediately, while both operand-selected
streams decode and render. Rust authenticates the surrounding `LDY/STA/LDA/STA` and
`BRA/LDA/STA/SEP/REP` instruction skeletons before interpreting these mutable operands.
Native special-file transfer now follows the same live operands. Extraction supplies two explicit
one-entry sources while retaining public order `GFX33`, then `GFX32`. Insertion preserves the
shared-bank runtime constraint: it tries bounded 32 KiB LoROM banks inside the selected allocation
range, stages both compressed RATS payloads, writes GFX33's low word and the shared bank once, then
writes GFX32's low word while requiring the same mapped bank. Both streams must reopen byte-exactly
before the checksum-repaired mutation is published. The authentic 2 MiB modified ROM passes this
repoint/reopen gate using its live inputs.

This distinction is now the primary automatic-profile boundary for pristine
SMW US revision 0. The Rust layout model must represent split three-plane
graphics pointers explicitly; treating `$003992` as a contiguous table would
silently combine bytes belonging to different files. Native UI auto-detection
must therefore select the split-plane backend until a verified expanded
graphics runtime is installed.

External revision profiles now carry that distinction canonically. Their ordinary `graphics`
table declares the low plane, count, and stride; `graphics.pointer_encoding=split_planes` adds
the high- and bank-plane offsets. Validation treats all three planes as independent metadata,
ROM audit reconstructs each address bytewise, and allocation protects every physical span. An
exact match to the authenticated pristine layout also enables native `GFX33`/`GFX32` directory
transfer using the recovered special-pointer operands; the table's `$33,$32` order is retained
internally while public filenames remain stable.

Expanded-profile auditing and transfer now model the adjacent enumeration semantics without
weakening ordinary pointers: `$000..$033` remain required, while an all-zero address in a later
expanded-table slot is an unused auxiliary/ExGFX sentinel. The installed native editor enumerates
nonzero `$080..$FFF` entries for extraction and canonical
`ExGFX80.bin` through `ExGFXFFF.bin` directory members for insertion. Sparse insertion retains the
actual table indices, preserves existing decompressed lengths, accepts new files only at the
recovered `$800`/`$C00`/`$1000` native depths, and saves every selected payload and pointer in one
checksum-repaired transaction. Raw transfer no longer coerces 2bpp or 3bpp files through 4bpp.

The adjacent Layer 2 pointer table is descriptor index 26 (`+0x68`): its installed headered offset
is `0x02E800`, hence logical offset `0x02E600`, with 512 contiguous 24-bit entries. Layer 2 storage
depends on the five-bit level mode. Object-storage modes retain a terminated header/object stream.
Tilemap-storage modes use generic decompressor selector 2, which dispatches to Lunar Magic's
terminated byte-run RLE—not LZ2 or LZ3. Legacy decoded streams are `0x360` bytes: entries
`0x000..0x1AF` become tile words `0x000..0x1AF`, and entries `0x1B0..0x35F` become words
`0x200..0x3AF`; unused words are zero. Newer `0x800`-byte streams contain separate `0x400`-byte
low/high planes and are interleaved into little-endian tile words. Rust decoding of the installed
level-000 and level-105 streams produces 2,048-byte payloads exactly equal to their exported MWL
Layer 2 sections (SHA-256 `67cb940c…55cfffe` and `5c5299db…f52927d`, respectively).
The Rust implementation now models these two storage classes in a focused module, supports
layout-explicit legacy and split-plane encoding, transactionally relocates/repoints the payload,
and carries the optional table through canonical revision profiles, allocation protection, and ROM
auditing. The legacy layout's high tile byte is profile metadata rather than an inferred default.
The typed MWL Layer 2 section additionally preserves its descriptor and source-address prefix and
reports the recovered storage flags and active bank through `lm-cli mwl`.

Per-level palette and ExAnimation pointer layouts must remain optional in an installed-ROM profile:
the Lunar Magic descriptor identifies marker-gated hooks for both, but the observed pristine-install
save cases leave their candidate tables uninstalled (`$FF`). A profile must not claim those tables
merely because descriptor addresses exist.

`LoadLevelPaletteDataPcOffsetTable` (`004812F0`) reads the marker from descriptor `+0x128` and
requires `$C2`; descriptor `+0x124` supplies the three-byte pointer table. Marker failure zeroes all
`0x209` resolved entries. The installed SMW-US descriptor's headered candidates are `$077770` and
`$077800` (logical `$077570` and `$077600`). Pristine ROMs contain `$FF`;
expanded shared-palette installation writes the marker/runtime and initializes
the pointer table to zero. A dynamic Lunar Magic 3.63
`-ImportCustomPalette` oracle changes the marker to `$C2`, writes table entry 0 as
`$10:8031`, and allocates the exact `0x202`-byte palette at logical `$080031`. Re-export proves the
MWL palette section is two provenance words, a backdrop word, and 256 BGR555 words. Those 256
stored words are circularly shifted left by one relative to TPL/editor order.

`BuildExAnimationPayloadPcOffsetIndex` (`004814A0`) gives the newer hook priority: descriptor
`+0x784` is tested for `$22`, then legacy descriptor `+0x5A4` is tested for `$22`. Descriptor
`+0x5A8` identifies code operands used to recover the active pointer-table address. The newer form
treats a slot as present when `raw_pointer & $FFFF00 != 0`; the legacy form uses
`raw_pointer & $FF0000 != 0`. Missing slots and an absent subsystem are distinct states, though both
become zero PC-offset entries internally. Candidate marker bytes in both captured ROMs are `$C2`
and `$E2`, proving neither hook is installed. A dynamic `-ImportLevel` oracle containing one
minimal compact record selects the primary hook: the headered physical marker at `$002590`
(logical `$002390`) becomes `$22`. Lunar Magic allocates its 512-entry table at physical `$081381`
(logical `$081181`); entry 0 is `$10:97E9`, which resolves to the 17-byte compact payload at
logical `$0817E9`. Re-export returns an MWL ExAnimation section containing the two provenance words
followed byte-for-byte by that same compact representation. Rust decodes it as the injected kind
`$01`, size mode 0, one frame, destination `$0100`, and frame word `$0600`.
The primary hook's long operand at logical `$002391` resolves to its allocated runtime at
`$0806B9`; the pointer-table operand is at runtime target minus `$86` and contains `$10:9181`.
The Rust `ChainedSnesPointerLocator` models this two-stage lookup so an allocator-dependent table is
discovered from installed code instead of being frozen to this oracle's incidental `$081181`
allocation.

A second combined-install oracle generated its MWL optional sections through Rust, then imported
that file with Lunar Magic. Both palette `$C2` and ExAnimation `$22` hooks were installed, the
Rust-created MWL reopened without semantic differences, and Lunar Magic re-exported both sections
successfully. In this allocation the hook target moved to logical `$0808C3`, its `-$86` operand
contains `$10:938B`, and the table therefore moved to logical `$08138B`. This independently proves
the table address is installation-specific and the chained locator is required.

The semantic-edit oracle then changed native palette color `$100` to BGR555 `$1234` and trigger 3
to `$07` using the shared Rust `LMMWLOE1` edit engine. Lunar Magic 3.63 accepted the generated MWL
under Wine and re-exported it. A relocation-neutral typed observation compared all 270 decoded
palette and compact-ExAnimation fields exactly. A strengthened 273-field observation additionally
uses the recovered revision size-mode table to compare the ordinary record's frame width and
source word `$0600` directly, while the container observation separately showed
the expected allocator/provenance pointer rewrites. Its ROM manifest reports 541 changed ranges,
19 added owned RATS allocations, no unexpected ranges, and zero semantic differences.

A subsequent frame-edit oracle started from the same Rust-generated combined MWL and applied the
shared semantic command `frame-replace 0 0 1234`. Lunar Magic 3.63 accepted the file under Wine,
installed it into a fresh copy of the same baseline ROM, and re-exported level 0 with source word
`$1234` intact. Its field-addressable observations compare exactly, while its independent ROM
manifest again reports 541 changed ranges, 19 added owned RATS allocations, no unexpected ranges,
and zero semantic differences.

The installed custom Layer 3 path is now independently recovered. `LoadLayer3TilemapGraphics`
(`00465080`) tests bit `$2000` in expanded-settings word 0 and passes word 1 to
`DecodeLayer3TilemapGraphicsRange` (`00464FC0`). That descriptor stores the GFX/ExGFX file in its
low 12 bits, a requested-length selector in bits 12–13, and a destination-offset selector in bits
14–15. The recovered constant tables map lengths to `$2000`, `$1000`, `$0800`, and zero bytes, and
destination word offsets to zero, zero, zero, and `$0800`; decoding clips the request at the
`$2000`-byte workspace boundary. The loader initializes all 4,096 words to `$38FC`, treats graphics
file `$07F` as the no-file sentinel after decoding the descriptor, and otherwise loads the exact
selected byte range at the decoded word offset. `InsertLayer3TilemapGraphicsFile` (`004690E0`)
consumes the same fields and tables. The main Layer 3 patch is an exact `$4C0` allocation, while the
expanded-settings runtime/table is an exact `$6E00` allocation whose table payload is reached
through descriptor entry `$70` at runtime offset `$1C0`.

The adjacent expanded mode state is also packed exactly. `GetPackedSpriteGraphicsSlotHighNibbles`
(`00464B00`) concatenates the high nibbles of expanded-settings words 12, 13, 14, and 15 into bits
0–15, then words 8, 9, 10, and 11 into bits 16–31; its reciprocal setter proves the ordering while
leaving every low twelve-bit graphics identifier independent. `ApplyPackedExpandedLevelModeFlags`
(`00464DF0`) gives one subset of those otherwise opaque flags a verified editor-row meaning when
packed bit 0 is set and the active Layer 3 setting is 1, or is 2 with object tileset other than 1.
It assembles an 11-bit signed row from packed bits 3–7 and 20–25. A type code assembled from bits
12–15 plus bit 26 clamps source rows beyond 30 for type 1 and types 6–17; all other types subtract
the ordinary twelve-row bias. `RenderLayer3TilemapCellAtCoordinates` (`004502C0`) independently
proves the row-30 clamp. The remaining packed flags feed the larger slot-assignment/painter
dispatcher and intentionally remain opaque until that dispatcher is authenticated.

`ConfigureLevelLayerSlotAssignments` (`004692B0`) now authenticates the color-composition subset of
that dispatcher. Its five entries each carry a source type, enabled flag, additive flag, half-color
flag, and Layer 3 priority selector; `RenderLevelEditorViewportRegion` (`004530A0`) copies those
flags into `RenderLayer3TilemapRegionToPixelBuffer`. With expanded mode enabled, packed bit 31
moves Layer 3 from the primary mode mask into the alternate source mask. On the primary route,
packed bit 30 replaces bit 2 of the active level-mode composition-table byte. The resulting table
byte—not the packed record's low byte—enables addition when bit 2 is set and bit 7 is clear, while
mask `$44` enables half-color when addition remains active. The alternate route never enables
addition and uses mask `$60` of that same adjusted level-mode byte for half-color. The pixel
renderer halves each source RGB channel before either its opaque write or saturating addition. A
live Lunar Magic 3.63 process opened on the retained installed SMW-US ROM supplied the complete
32-byte primary-source, alternate-source, and composition tables at `$0091F330`, `$0091F350`, and
`$0091F370`. Mode `$00` independently reproduced the live source, enabled, additive, half-color,
and priority arrays byte-for-byte. The dispatcher can place Layer 3 in slot 0, 2, or 4 and, when
legacy-header byte 2 bit 7 requests a priority split, duplicates it across two slots with selectors
1 (low-priority tiles) and 2 (high-priority tiles).

`RenderLevelEditorViewportRegion` copies every active slot's additive byte to `DAT_0060028D` before
dispatching Layer 1, Layer 2, or Layer 3. `RenderMap16TileToPixelBuffer` proves the Layer 1/2 path:
ordinary nontransparent source pixels saturating-add per RGB channel, while its averaged-display
cells first shift each source channel right once and then saturating-add instead of averaging with
the destination. The cache renderer visits each final Map16 coordinate once, so overwritten object
paints do not contribute multiple times.

`tools/lunar-magic-layer-slot-audit.sh` now makes this dispatcher evidence reproducible. Its helper
stages the three active table values and four packed-mode modifier globals atomically, invokes
`ConfigureLevelLayerSlotAssignments` inside the live 32-bit process, and captures all five output
arrays before the editor loop can restore the selected level. The retained matrix covers 20 valid
modes × two legacy priority states × the base plus four expanded bit-30/31 states = 200 cases.

The retained Wine ROM also proves four direct hooks into the `$4C0` main payload. The first three
are now clean-room runtime contracts: logical `$00201F` replaces
`LDA $1BE3; BEQ +$20; DEC A` with `JSL entry; BEQ +$1F`; that entry begins at payload offset zero
and reaches the recovered vanilla fallback at internal offset `$0C`. Logical `$002153` replaces
`LDA #$06; STA $12` and targets payload offset
`$480`, whose negative `$7FC01A` path adjusts the stacked long-return address by three bytes before
returning, while the ordinary path preserves the replaced immediate/store sequence; logical `$0094B6`
uses a JML entry at payload offset `$4A0` plus an injected RTS continuation. A fourth hook at
logical `$02C40C` targets offset `$417`; its negative/legacy path discards the JSL return and
redirects through `$05C414/$05C494` according to `$1403`, while the custom-mode path tests `$145E`
and returns directly when its low bit is set. The Rust `lm-snes` builder and `lm-profile` fragments
model these as checked labels and relocations. A composed bundle rebases all four hook targets but
remains useful as a clean-room behavioral decomposition. The complete Wine source payload has now
also been recovered from `005B6178`: its 75 allocation-relative relocations, 64-entry dispatch
table, `$13D7` instruction rewrite, and four external hooks reproduce the entire retained `$4C0`
allocation. Expanded-settings installation is no longer a
Layer 3 blocker: its complete `$6E00` allocation, fixed hooks, relocations, checksum update, and
transactional rollback path are implemented independently in `lm-profile`.

The compatibility family is also independently installable now. `InstallLayer3CompatibilityBridge`
(`0046DC10`) owns a `$20`-byte bridge allocation and immediately invokes
`InstallLayer3AuxiliaryDispatchRuntime` (`0046DAC0`), which owns a second `$20`-byte allocation.
Clean-room 65C816 generation reproduces both Wine payloads exactly, including the auxiliary entries
at offsets zero and `$0D`. Seven fixed/allocation-relative writes cover the overlapping bridge JML,
its preserved comparison continuation, two branch displacements, the coordinate-path rewrite, and
the two auxiliary JSL hooks. The Rust installation is one checksum-valid, failure-atomic undo step.

The standard-LoROM resource selected by `InstallLayer3MainRuntime` (`0046B390`) is now decomposed
as well. Its `$3D0` allocation contains a zeroed `$200`-byte level workspace, a typed 32-word level
offset table, and `$190` bytes of runtime code/ownership marker. Wine changes only seven operands:
five low-word calls to the allocation's shared helper at offset `$3A0`, one long reference to the
allocation base, and one to base plus `$200`. `smw_us_v1_layer3_main_runtime_payload` represents all
seven as allocation fixups and reproduces the complete retained payload after resolution.
Eight external JSL/JML hook sites are also catalogued with pristine preconditions and independently
verified against the Wine allocation address. Two sites intentionally share entry `$240`; the
remaining entries are `$26B`, `$289`, `$29D`, `$2AF`, `$2BF`, and `$2D7`.
The live Wine descriptor at `$005E9DE8` additionally resolves six allocation-independent writes:
three coordinate-accumulator routines at logical `$06A963/$06A9D6/$06A9EF`, a 13-byte selector
rewrite at `$003F3C`, and one-byte adjustments at `$003F36/$02D8FC`. All six have exact pristine
preconditions and full-range Wine comparisons in `lm-profile`.
The descriptor's `$29F..$2A6` loop also widens eight engine masks from `$01` to `$3F`. Together
with the payload and eight allocation-relative entry hooks, these writes form one transactional
main-runtime installation with checksum repair and single-step undo.

`InstallLayer3ExtendedRuntime` (`0046DDE0`) selects PE resources `$206/$207/$208`; the standard
LoROM path uses the `$370`-byte `$206` resource. Its retained allocation begins at logical
`$085AAE` and ends with `XSPRITE-GEN1          LM\x01\x01`. Six allocation-relative operands
target entries `$95`, `$00`, `$08`, `$00`, `$08`, and `$273`; two additional operands are
revision-mapped to the SMW US layout. Descriptor entries `$2B3..$2CD` and `$2A7..$2AA` resolve to
22 non-overlapping fixed/JSL/JML writes. `lm-profile` reproduces the full Wine payload and every
write, installs them as one checksum-valid transaction, and proves late-failure rollback and
single-step undo.

The `$4C0` main patch, `$3D0` main runtime, `$20+$20` compatibility family, and `$370` extended
runtime now compose into a five-allocation, 55-write installation plan. Payload indices are rebased
without freezing allocator results, and the entire family commits or rolls back as one project
revision. Both the CLI (`layer3-install INPUT OUTPUT`) and cross-platform application shell
(`layer3-install` on an open project) expose this identity-checked workflow.
The user-facing workflow groups that runtime plan with the separately aligned expanded-settings
plan. This preserves Lunar Magic's `$087FF8` RATS header and `$088000` table payload while committing
all six allocations as one history operation; a failure in either allocation policy rolls back the
other plan as well.

The first hook is now recovered further through payload offset `$6A`. Before entering its
table-driven helper region, the custom-mode arm derives state from `$7FC01A/$7FC01B` and `$145E`,
updates the Layer 3 bit in direct-page `$40`, conditionally enables the layer in `$0D9D` and
`$212C/$212E`, and initializes `$146A`, `$146C`, and direct-page `$01`. The independently generated
`smw_us_v1_layer3_main_dispatch_setup_fragment` matches all `$6A` installed bytes at this boundary;
its following JML is a typed cross-component relocation into the generated selector dispatcher.

The two indexed scroll tables at payload offsets `$357` and `$397` are now semantically decoded.
Each contains 32 entries selected from a small formula set: base only, base plus the camera
coordinate, divisions by powers of two, an axis-specific dynamic calculation, or division by five.
The divide-by-five routine selects the SNES or SA-1 arithmetic registers from its execution
context; horizontal mode 26 supplies the additional divide-by-64 case. The revision-specific
Rust `Layer3ScrollFormula` tables are checked entry-for-entry against both installed pointer tables;
simple formulas use explicit 16-bit wrapping, and dynamic cases use separately modeled engine
state.

Those dynamic cases now have explicit RAM-state transition models. The horizontal helper advances
the `$145C/$1458` fixed-point phase, sign-extends `$17BD`, updates `$22`, conditionally derives
`$26`, and advances `$17BF`; its `$0BE6` high-bit path suppresses one advance, and the apparent
cross-axis branch for nonzero `$9D` is confirmed to select `$146C` into `$24`. Vertical pointer
`$9C21` similarly combines `$145D/$145A`, `$17BC`, `$0BE7`, and `$24`. Pointer `$9C59` is a distinct
camera/clamp helper: it updates `$146C`, clamps or wraps the displayed position, derives `$28`, and
on one `$190D` path executes twice for secondary scratch effects before restoring phase, base, and
displayed scroll from the first execution. The Rust models retain unresolved engine fields by RAM
address and reproduce all 8/16-bit wrapping rather than assigning speculative names.

Runtime lowering no longer depends on the installed bank-relative pointer tables. `lm-snes` now
resolves checked 16-bit local branches and relocatable local `JSL`/`JML` labels, with explicit
out-of-range and unbound-label failures. The generated `Layer3ScrollHelperLibrary` provides direct
local entry offsets for every ordinary horizontal and vertical selector. Shift-based formulas are
emitted independently, and divide-by-five contains both the `$4204/$4206/$4214` SNES path and the
`$2250/$2251/$2253/$2306` SA-1 path. Its remaining targets are typed as one horizontal and two
distinct vertical dynamic helper families.

All three dynamic targets are now lowered as well. The generated horizontal routine contains its
normal, Layer-2, scratch-`$26`, `$17BF`, reset, and cross-axis returns. The two vertical entries
share a relocatable local camera helper and preserve the double-step stack restoration. A width
audit corrected an important detail: `LDX $0BE6/$0BE7` tests low-byte bit 7 in 8-bit index mode,
while the reset action clears word bit `$8000`; the high-level state model and generated code now
agree on that asymmetric operation. Every one of the 64 selectors resolves to a bounded generated
entry, and all local `JSL`/`JML` operands are emitted as payload-relative fixups.

The first-hook selector continuation is now generated end-to-end. It normalizes and stores both
selectors, applies the phase-constant override rules, calls all effective horizontal and vertical
helpers, snapshots `$22/$24` when requested, initializes `$1458/$145A/$145C/$145D`, reproduces the
width-sensitive `AND #$F0F0`/`TSB $0BE6` tail, commits `$13D5`, restores 8-bit accumulator mode,
and returns to the setup routine's offset `$06`. Setup, dispatcher, and helper library are flattened
into one self-contained payload with every relocation rebased locally. The guarded bundle therefore
no longer lists the main-mode path or localized helpers as missing.

The retained `$6E00` expanded-settings allocation is now separated from the code that references
it. Its payload contains no executable bytes in this fixture: offsets `$0000..$2CFF` are `$FF`,
followed by exactly 520 32-byte records through offset `$6DFF`. The first 512 are standard level
records; record 512 begins at offset `$6D00`, matching `InstallExpandedLevelHeaderRuntime`'s special
pointer. The recovered initializer writes `$007F` to all words, then `$FFFF` at word 8 and
`$002B/$002A/$0029/$0028` at words 12–15. The current-layout normalizer preserves word 1, derives
word 0 from old word 13 plus old bit `$8000`, shifts old words 3–12 into words 2–11, converts
`$FFFF` to `$007F` and masks other applicable references to 12 bits, and restores the four trailing
defaults. `SmwUsV1ExpandedSettingsAllocation` implements this exact shape and round-trips the Wine
allocation byte-for-byte. The remaining family is therefore the separate hook/pointer installer,
not hidden machine code inside the allocation.

The fixed first-fit address is not itself an ownership marker. A retained ordinary level-save ROM
has a valid `$8000`-byte RATS allocation at `$087FF8` while all expanded-settings runtime
destinations remain pristine. The shared profile loader therefore treats that combination as
uninstalled defaults, accepts `$6E00` only after validating its fill prefix and records, and still
rejects a wrong-length block once the runtime destinations prove installed ownership. Both the
interactive shell and `lm-cli` overworld-settings import/export routes now delegate to this single
decoder instead of performing weaker local `STAR` tests. A process test expands a supported ROM,
places the unrelated `$8000` allocation, and requires CLI export to produce the exact defaults; a
terminal test repeats collision and genuine-owned reads with and without copier headers, proves
exact settings, and verifies that export is physically non-mutating.

The original allocator also relocates this family rather than failing when that ordinary block
occupies the preferred bank tail. Importing the retained Layer 3-settings MWL into the collision
ROM under Lunar Magic 3.63 preserves the `$087FF8` owner, places the new RATS header at `$090000`,
uses payload `$090008`, and moves the record table to `$092D08`. The runtime base operand becomes
SNES `$128008` and the table operand becomes `$12AD08`. This exposed a tenth allocation fixup in
descriptor `$172`: its immediate at copied offset `$16` holds the low byte of allocation + `$2D00`
independently of the three-byte table operand. The relocation resolver now models that byte, accepts
the original's zero-filled free tail as well as `$FF`, derives installed storage from the
authenticated runtime operand and exact `$6E00` RATS owner, and makes canonical revision profiles
follow that resolved table. `lunar_magic_and_rust_match_relocated_expanded_settings_owned_bytes`
compares the complete allocation and every fixed runtime write against fresh Lunar Magic runs for
both copier-header forms; the native application, CLI process, semantic reopen, checksum, and
byte-exact Undo/Redo gates cover the same collision.

The restored Ghidra project now separates the historical record transformations used by
`LoadLevelHeaderRecordWithVersionUpgrade` (`00462E10`).
`UpgradeExpandedLevelHeaderRecordLayout` (`00461E80`) moves old words 1–9 to current words 5–13,
initializes words 1–4 and 14–15 to `$007F`, and retains only word 0's `$8000` bit before applying
that same default. `NormalizeLevelHeaderRecordReferences` (`00461F90`) independently maps `$FFFF`
to `$007F`
and masks words 2–10 except non-reference word 8. `NormalizeExpandedLevelHeaderRecord`
(`00461ED0`) is the distinct field-reordering pass already modeled by the current normalizer.
`SmwUsV1ExpandedSettingsRecordGeneration` now dispatches the exact composition for the four load
branches, with exhaustive source/destination-word and untouched-field tests. Whole-ROM migration
is not inferred from these record rules. An official Lunar Magic 2.42 executable and a ROM created
and saved by that executable now provide the authenticated `$0102` generation fixture. Its marker
is exactly `4C 4D 02 01`; descriptor `$173+$33` resolves an exact `$6E00` RATS owner, and every
fixed runtime destination matches the current family except the version marker, allocation
operands, and historical descriptor `$220` body. Rust authenticates that complete immutable
boundary, normalizes reference words 2–10 except word 8 across only the 512 standard records,
preserves all eight trailing records byte-for-byte, reclaims the old owner only in staging, installs
the current runtime/table, repairs the checksum, semantically reopens, and records one exact
Undo/Redo operation. Corrupt marker, runtime, pointer, owner, length, or fill-prefix evidence rejects
before mutation.

The older generation-1.01 boundary is now independently authenticated from Lunar Magic 2.22.
Unlike 2.30 and later, it publishes no marker at `$07F15C`; its companion graphics-table marker is
`4C 4D 01 01` at `$06FF37`, and its expanded-header RATS owner is only `$6D00`: `$2D00` fill plus
512 records, with no Layer 3 special-record tail. Two real 2.22 saves prove both payload `$0801E0`
(SNES `$1081E0`) and a forced relocated payload `$088000` (SNES `$118000`). Only six operand
groups change: the record-table long address, its low-word-plus-two/bank publications, allocation
base, and allocation base plus one. Rust derives and verifies all six for the live owner, masks only
those operands for SHA-256 authentication of the complete immutable runtime/hook family, normalizes
the 512 records with the recovered reference-only pass, initializes the eight current special slots,
and atomically replaces the old owner with current `$6E00` storage. Corruption and byte-exact undo
are tested against both placements. The still-earlier pre-expansion `$0100` record-layout family
remains evidence-gated work.

Instruction-level inspection of `CheckLegacyGraphicsTablePatchState` (`004609D0`) and
`CheckExpandedLevelHeaderPatchState` (`00460A30`) also recovered the previously omitted generation
publication. Both read four bytes immediately before descriptor `$69` at `$07F15C`, require `LM`
in the low word, and compare the high word against `$0102` or `$0103`. Current installation writes
exact bytes `4C 4D 03 01`. That marker is now an expected-byte-guarded member of the atomic runtime
plan rather than an unowned side effect. The live relocated Wine reciprocal gate and the complete
fixed-write comparison both now cover it, closing a real byte-level discrepancy in Rust-created
ROMs.

The pointer installer is now represented as typed, non-installable relocation evidence. The
recovered `PatchExpandedLevelHeaderTablePointers` loop patches descriptor entries `$35..$39` at
operand offset `$6C` to fixed runtime entry `$70`. The allocation-dependent portion of
`InstallExpandedLevelHeaderRuntime` publishes the record-table address (allocation + `$2D00`) at
entry `$172+$0F`, the allocation base and base+1 at `$1DB+$37/$3D`, and the base again at
`$173+$33`. These relationships live in `expanded_settings_runtime.rs`; they remain separate from
an executable patch plan until every copied code range and clean-ROM expected byte is verified.

All twelve copied runtime ranges are now cataloged by descriptor entry, embedded-template address,
length, and post-copy mutable operand spans. Correcting for the live descriptor's physical
copier-header offsets showed that the Wine-produced ROM matches every embedded template byte outside
those spans. The Rust verifier enforces that invariant across entries `$69`, `$72`, `$172`, `$173`,
`$19F`, `$1DB`, `$213`, `$215`, `$216`, `$219`, `$21C`, and `$220`. It records addresses only and
does not redistribute the embedded payloads. Two complete relocation-free blocks are already
independently generated with the 65C816 builder: `$213` preserves flags around vanilla `$F9F7`,
restores X from `$13C6`, and returns `$18`; `$219` compares `$1F11/$1F12`, conditionally writes
`$0C` to `$0100`, and continues at `$05DBF2`.

Generated coverage is now four of twelve blocks. `$173` implements the `$42` state path, scratch
pointer capture, vanilla `$F8CB` helper call, and allocation-indexed load. `$216` publishes the
special-record low word and bank into `$7FC006/$7FC008`, commits state `$42`, and clears
`$7FC00B`. Both generators accept validated 24-bit SNES addresses. Tests prove the placeholder
forms against the executable templates and the resolved `$11:8000/$11:ED00` forms against the
Wine-installed ROM.

Coverage is now five of twelve: descriptor `$72` is generated as two bounded entries. Its primary
entry runs vanilla `$FD80/$F9E0/$F840/$F8B8` helpers, resolves `$FB`, clears the four
`$0105,X` state slots, and publishes zero to `$7FC009`; its secondary offset-`$50` entry gates
`$F840` on `$7FC00B` bit 0. The complete `$60`-byte padded block matches both the embedded template
and Wine ROM without relocations.

Generated coverage has reached eight of twelve blocks. `$69` now reproduces its `$0A/$4B` stack
selector dispatcher, `$42` state path, `$FC/$FD` reset behavior, vanilla helper calls, data-bank
handling, and embedded eight-byte X remap table. `$19F` supplies the indexed table loader and two
scratch-pointer entries sharing a mapped helper call. `$172` consumes the expanded record-table
address, publishes the selected record through `$7FC006/$7FC008`, updates `$7FC009`, and preserves
the disabled `$FF` state path. Each address-dependent generator matches both the executable's
placeholder form and the Wine fixture's resolved form.

Coverage is now ten of twelve blocks. `$1DB` dispatches among vanilla tables, the expanded
allocation, a compatibility table, and the `$007F` sentinel before its register-restoring
trampoline. `$215` models the installer-configurable opcode explicitly (`RTS` in the embedded
template, `CLC` in the Wine-installed expanded mode), normalizes the active pointer, performs four
DMA setup iterations, restores `$7FC006/$7FC007`, and includes its fixed word table. Both complete
generated blocks match their respective oracle variants byte-for-byte.

Coverage is now eleven of twelve blocks. `$220` is independently generated as focused header,
field-setup, transfer-arithmetic, bit-extraction, and DMA-register emitters plus its fixed tables.
It decodes `$7FC01A/$7FC01B/$7FC01C`, updates `$145E`, resolves table-driven transfer parameters,
sets the active flag, and exposes the two compact helper entries. The complete `$150`-byte block
matches both relocation-free oracles exactly. Only descriptor `$21C` remains.

Generated coverage is now twelve of twelve blocks. The final `$21C` transfer runtime models its
alternate entry, pointer publication, indexed helper loops, two DMA paths, expanded transfer pairs,
video setup, compact record helper, DMA commit, wait helper, and fixed table. Its configurable
special-record address, helper call, and continuation opcode reproduce both the executable
placeholder and Wine-installed variants byte-for-byte. Two branches intentionally target bytes
inside existing instruction encodings; the assembler represents those overlapping-code labels
explicitly. The runtime bodies are complete, while clean-ROM allocation, hook installation, and
compatibility composition remain separately guarded as an unfinished installation component.

The twelve generators are also composed through one revision-specific runtime-family API. It
resolves the allocation base, record table, special record, mapped table/helper, continuation mode,
descriptor identities, and all twelve logical destinations together; checked validation rejects
overflowing or overlapping destinations. A second boundary converts that family into exact-guarded
fixed-ROM writes. Tests compare every expected range against the pristine pre-install ROM (all
`$FF`) and every replacement range against the Wine-installed ROM. The settings allocation,
descriptor pointer publications, and remaining hooks are intentionally not implied by these writes
and keep the expanded-settings installation guard active.

The allocation binding is now a separate typed phase rather than a fixed Wine address assumption.
Nine fixups cover the record-table long operand and repeated bank at `$172+$0F/$1F`, allocation
base at `$173+$33` and `$1DB+$37`, base+1 at `$1DB+$3D`, and the split special-record low-word/bank
publications at `$216+$09/$12` and `$21C+$39/$42`. The resolver checks component identity, operand
bounds, addend overflow, and the 24-bit SNES bus. A placeholder runtime family rebound after
allocation compares exactly with the directly generated `$11:8000` Wine family. The matching live
Ghidra installer at `00460A90` is annotated with these exact sites.

The first three direct runtime hooks are now independently composed as well. Descriptor `$174`
replaces logical `$0283B8` bytes `AD 25 19 C9 09` with `JSL $0F:F7F0; NOP`, targeting `$172`.
Descriptor `$214` replaces `$001471` bytes `AE C6 13 A9 18` with `JSL $0F:F9C0; NOP`, targeting
`$213`. Descriptor `$217` replaces `$002140` bytes `85 20 E2 20` with `JSL $0F:FAB0`, targeting
`$216`. The generator derives targets from the runtime layout, uses Lunar Magic's low-bank LoROM
mirror, and rejects unmappable destinations. Exact expected and replacement slices match the
pristine and Wine ROMs respectively. The combined fixed-write family now contains twelve runtime
bodies and these three hooks.

Instruction-level recovery of the following apparent “sequential” relocation calls showed that
nearly all are site/target pairs within those twelve generated blocks, rather than additional
external writes. The one external mutation is descriptor `$21A+1`: logical operand `$021DFE`
changes from `F2 DB 05` (`$05:DBF2`) to `F0 FA 0F` (`$0F:FAF0`), retargeting the existing `JSL`
to descriptor `$219`. It is now a fourth target-derived exact-guarded hook write.

The fixed descriptor `$70` family is now generated and installable too. Descriptor `$70` resides at
logical `$06F0F0` and snapshots direct-page words `$57/$59` into `$FA/$FB` before returning; its
16-byte padded body independently matches Lunar Magic's embedded template. Five operand sites at
logical `$06A4C1/$06C206/$06CE06/$06DA06/$06E906` (descriptor entries `$35..$39` plus `$6C`)
each change from `E3 B3 0D` (`$0D:B3E3`) to `F0 F0 0D` (`$0D:F0F0`). The persistent descriptor
`$71` base hook at logical `$002A50` changes `A2 03 B5 04` to `22 80 F7 0F`, entering the final
descriptor `$72` runtime at `$0F:F780`. The intermediate `$72` template and relocations written by
`InstallBaseExpandedLevelHeaderHooks` are intentionally omitted from the final patch plan because
`InstallExpandedLevelHeaderRuntime` overwrites that complete range before the transaction finishes.
The exact-guarded fixed-write family now contains 23 non-overlapping entries: twelve main runtime
bodies, descriptor `$70`, five `$70` pointer publications, and five persistent hooks/relocations.

Those components now compose into one executable clean-install transaction. The plan expands a
pristine ROM through logical `$090000`, places the RATS header at `$087FF8` and its `$6E00`
settings payload at `$088000` (`$11:8000`), applies all 23 guarded writes, resolves all nine
allocation operands, repairs the checksum, and commits one undo batch. The shared relocatable-patch
engine now supports contiguous 24-bit, split low-16, and bank-byte operands, including explicit
low-bank LoROM mirrors; the external `LMPAT001` format remains deliberately restricted to its
original contiguous long-address contract. Differential tests match every transaction-owned byte
against the Wine result, reopen record `$207` through the semantic project API, and prove that both
undo and a late hook-precondition failure restore the exact original ROM and history.

The allocation constructor also models the recovered special-domain defaults rather than treating
all 520 records as identical. Records `$000..$1FF` use the ordinary initializer, `$200..$206` use
the repeated special record
`0014,007F,007F,007F,001E,0008,001D,001C,001D,001C,000F,0010,002B,002A,0029,0028`,
and `$207` returns to the ordinary initializer. In the retained Wine oracle, the subsequent MWL
import changes only ordinary record `$000`; every other allocation byte matches the clean
transaction exactly.

The transaction is exposed through both portable frontends. `lm-cli expanded-settings-install
INPUT_ROM OUTPUT_ROM` verifies SMW US revision 0 LoROM identity, installs and semantically reopens
the table, and publishes a create-new output without touching the source. The application shell's
`expanded-settings-install` command performs the same operation as one revision-bound project edit,
so ordinary `undo`, `redo`, and `save-as` behavior applies. Built-binary tests run both workflows on
the retained pristine ROM, verify checksum-valid output and the RATS allocation, preserve the
source, and reject replacement of an existing CLI destination.

The adjacent legacy `DecodeGraphicsRemapCommandStream` (`004648F0`) has also been recovered at the
instruction level. Each command starts with four bytes. Header byte 0 bit 7 terminates; its low
seven bits and byte 1 form a 15-bit destination word. Header byte 2 bits 0–5 plus byte 3 form a
14-bit raw length, bit 6 selects a two-byte repeated value instead of literal payload bytes, and bit
7 selects a 32-word rather than one-word destination stride. Literal lengths are raw+1; repeated
output lengths are raw+2. Destinations wrap with `$7FFF`. Odd literals replace the low byte of the
next destination word, while odd repeats copy the repeated value's low byte into that word's high
byte. Parsing stops at a terminator or after the first complete command reaching/crossing `$8000`
consumed stream bytes. Lunar Magic applies these writes to a private `$8000`-word scratch map and
restores the caller buffer, so the clean-room Rust codec/interpreter exposes generic scratch-map
semantics without assigning an unproven Layer 3 side effect.

A Rust-generated MWL enabled that path with file `$028`, length selector 2, and destination selector
0, then Lunar Magic 3.63 imported and re-exported it under Wine. All 24 decoded expanded-settings
fields matched exactly. The retained `rust-mwl-layer3-settings-import` manifest records 584 changed
ranges, 21 added owned RATS allocations, no unexpected ranges, and zero semantic differences; it
also independently contains the exact `$4C0` and `$6E00` allocations above.

All positive cases are retained under `oracle-work/lm363/pristine-us/` with manifests that verify
the complete before/after ROM hashes, semantic MWL observations, changed ranges, and newly owned
RATS allocations. The original supplied ROM remains byte-identical at SHA-256
`0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b`.
- `g_pCurrentRenderingLevelObject` (`006008fc`)
- `g_bTrackRenderedObjectCells` (`005f16e6`)
- `g_dwLevelObjectRenderBoundsFlags` (`0060aeac`)
- `g_dwLevelMap16PageStride` (`00608f48`)
- `g_bLevelEditorStateLoaded` (`00e2782a`)
- `g_bUnicodeWin32ApisAvailable` (`007592eb`)
- `g_pApplicationInstance` (`00e2780c`; Win32 `HINSTANCE` represented as a pointer for project naming-policy compatibility)
- `g_pLanguageResourceModule` (`00e27a58`; Win32 `HMODULE` represented as a pointer)
- `g_dwSecurityCookie` (`005e0284`; MSVC `/GS`, not Lunar Magic domain state)

## Save-level transaction observations

`SaveLevelToRom` serializes multiple independently allocated payloads. Layer 1 and Layer 2 object streams are limited to a single `0x8000`-byte LoROM bank and are terminated by `0xFF`. Optional payloads include palette, ExAnimation, and sprite data. Existing RATS blocks can be reused when byte-identical deduplication is enabled. PC offsets are converted into mapper-specific 24-bit SNES pointers for LoROM, ExLoROM, and SA-1 variants before level tables are updated.

The installed format-`$103` Layer 2 path is now also verified against an authentic Lunar
Magic-modified ROM rather than only imported MWL and pristine expansion fixtures. A native canvas
insertion allocates and RATS-tags a grown object stream in the modified ROM's zero-filled expanded
space, changes only the selected Layer 2 pointer, and leaves the selected Layer 1/sprite pointers
and a neighboring Layer 2 pointer exact. Lunar Magic 3.63 reopens and exports the expected semantic
stream, confirming that the recovered descriptor/pointer interpretation and allocation boundary
agree with the original editor.

The supported runtime-variant boundary is now explicit rather than inferred from format `$103`.
`every_legacy_generation_migrates_across_both_copier_header_variants` exercises authenticated
formats `$100`, `$101`, and `$102` with both absent and present copier headers. Each case installs
the exact `$103` hook, applies its generation-specific descriptor conversion, preserves every byte
of a present prefix, and traverses byte-exact full-image Undo and Redo. Together with the retained
current-runtime editor oracle, this covers the supported SMW-US LoROM generation/header product.

The reciprocal authentic Layer 1 lifecycle gate now drives both `RelocateOrdinaryPosition` and
record removal through the native editor before saving. The recovered relocation algorithm's
screen-stable reorder and synthesized screen transitions survive Lunar Magic 3.63 reopen/export
exactly, and deleting a second ordinary record preserves that resulting control-stream shape.
Only the selected Layer 1 pointer changes; Layer 2, sprites, and the neighboring level remain
untouched.

The recovered object walker distinguishes command-zero controls by parameter, not command ID
alone. Parameters `$00–$03` are screen-exit/jump/control forms, while `$04+` participate in normal
position walking as extended objects. Native canvas insertion previously imposed the broader and
incorrect `command != 0` test; the stream model and both Layer 1/Layer 2 canvas paths now share the
recovered boundary. A modified-ROM Lunar Magic 3.63 re-export proves placement plus a subsequent
extended-selector edit remains canonical.

The recovered standard/extended definition tables and resolved OSC selectors are now the native
selection authorities for Layer 1 and object-backed Layer 2. The extended catalog enumerates only installed `$04+`
definitions, layers the active object-tileset substitutions over the shared table, and uses the
resulting pattern for preview. In particular, object tilesets 0/8 replace selector `$17` with
Map16 `$12D`, matching `install_lunar_magic_tileset_extended_objects`; other tilesets retain the
shared definition. OSC selection additionally materializes the command-declared native width and
retains its extension bytes as placement provenance. Both layers dispatch their chosen standard,
extended, or custom record through the same positioned-stream insertion algorithm.

The structure allocator consistently accounts for the 8-byte RATS header. `FindDuplicateRatsPayload` reads and compares only validated candidates and rejects sizes over `0x8000` for level payloads.

## Native overworld path-link tables

The static and live Wine analysis now ties `LoadPackedLevelDescriptorTables` (`004B5200`) and
`CommitPackedLevelDescriptorTables` (`004B5450`) to Lunar Magic's three editor arrays. The source
and return endpoint arrays each contain at most 128 packed five-byte `OverworldEndpointRecord`
values; a third array contains two engine target-coordinate bytes per entry. The expanded format
allocates the planes consecutively as `5*N + 5*N + 2*N` bytes and relocates all eight operand
references. On an unmodified SMW US revision-0 ROM the fallback loader reads fourteen entries from
PC `$21964`, `$219AA`, and `$219F0`, corresponding to SNES `$049964`, `$0499AA`, and `$0499F0`.

The Rust implementation keeps this special-link table separate from the higher-level `LMOWPATH`
graph because the two formats do not encode the same topology. `LMOWLN1` losslessly interleaves one
record for editing/export, then reconstructs the exact three ROM planes. The pristine 168-byte
oracle round-trips without differences; direct save stages all planes and checksum together and
publishes one undo entry.

Live port-8089 revalidation of `OverworldPathLinkDialogProc` (`00537690`) and its four endpoint
helpers recovers the original directional editing transform without inventing a stored direction
field. `PopulateOverworldDirectionCombo` (`005362E0`) installs `Up`, `Down`, `Left`, `Right` in
that exact order, producing ordinals 1 through 4. `EncodeOverworldDirectionalPoint` (`00535850`)
and `DecodeOverworldDirectionalPoint` (`005358B0`) both apply wrapping endpoint deltas of Y+8,
Y-8, X+8, and X-8 respectively. `EncodeOverworldExitNodeWithDirection` (`00535910`) applies the
opposite sixteen-pixel deltas Y-16, Y+16, X-16, and X+16. The dialog derives this direction from
endpoint geometry, uses an absent all-`$FF` return endpoint for a one-way path, and clears stale
adjacent source, return, resolved, and preview records before installing a replacement. Rust now
models the exact ordinals and both transforms as `OverworldPathDirection`, transposing those raw
deltas through the native Y/X planes into its public semantic X/Y endpoint order; direction remains
transient editor state so the lossless three-plane ROM format is unchanged. A retained live Wine
transition changes native link 4 from Left to Up and one-way: raw source `$00A0,$00D8` becomes
`$0098,$00E0`, the return endpoint becomes all `$FF`, and both target bytes remain exact.

The adjacent two-click `Link Exit Path Tiles` workflow does not use that same validation table.
`ValidateSelectedOverworldExitTile` (`0053A2B0`) disassembles to a stride-two Layer 1 type read at
`$00B14F68` followed by the independent lookup at `$00B14168`. A live 256-byte read proves that
lookup accepts exactly types `$5A`, `$5B`, `$5F`, and `$82`; none overlap the thirteen Submap Exit
Tile Settings types. `HandleOverworldExitTileLinkClick` (`0053A350`) uses the first click to select
the endpoint family and the second click to open `Link Exit Path Tiles`. Its owner-drawn controls
retain source index `$0066`, return index `$0069`, and direction `$0068`. The Rust model exposes
both exact predicates separately so later gesture integration cannot silently route one tile family
through the other dialog.

The expanded form is now reproduced as well. Live Wine inspection resolved descriptor `+0x920` to
headered physical `$21C35`, hence logical hook `$21A35`; the pristine five bytes are
`A9 1A 00 85 02`, replaced by `JSL runtime; RTS`. Ghidra's embedded template at `005C36E0` is
exactly 112 bytes and ends with marker `4C 4D 00 01`. The count-minus-one immediate is at `+6`,
the corresponding `(count-1)*5` immediate is at `+$0B`, and long operands at
`+$11/+$1A/+$20`, `+$2C/+$33/+$3A`, and `+$48/+$52` address the three fields of each five-byte
source record, the three destination fields, and both target bytes. Rust validates those
relationships, installs the runtime and contiguous `5N+5N+2N` table as separate RATS blocks, and
updates or relocates the table failure-atomically with checksum repair and semantic reopen.

## Native overworld warp/exit-link tables

`LoadFourLevelMetadataTables` (`004B47A0`) and `CommitFourLevelMetadataTables` (`004B4D80`) were
initially suspicious because of their provisional names, but their xrefs prove they are the
overworld warp/exit endpoint store. `PopulateOverworldWarpEndpointCombo`,
`CopyOverworldWarpSourceEndpoint`, `CopyOverworldWarpReturnEndpoint`, and
`DrawOverworldExitIndexOverlay` consume four 256-word editor arrays. A valid entry consists of a
source packed-vertical word, source horizontal-tile word, destination packed-vertical word, and
destination horizontal-tile word. `0xFFFF` is retained as the absent sentinel. Commit trims a
trailing suffix where all four values are absent and allocates four consecutive `2*N` planes when
more than the vanilla capacity is required.

Live read-only LLDB analysis of Lunar Magic 3.63 under Wine found
`g_pActiveRomLayoutDescriptor` (`009203C8`) pointing to `005E9DE8`. Descriptor fields
`+0x4A8/+0x4AC/+0x4B0/+0x4B4` contained headered file offsets
`$20631/$20667/$2069D/$206D3`. Removing the loaded ROM's 512-byte copier header yields logical
offsets `$20431/$20467/$2049D/$204D3`, exactly four adjacent 54-byte planes. These bytes match
the static fallback loader's four `0x36`-byte reads and the supplied pristine ROM.

Rust intentionally exposes the vertical word as `packed_vertical` rather than inventing an
unverified submap mask. `LMOWWR1` interleaves eight lossless bytes per link, while native ROM I/O
reconstructs all four planes. Focused tests cover partial words, plane-count disagreement,
256-entry limits, exact pristine re-encoding, overlap rollback, checksum repair, and undo. A
built-CLI export/import of all 27 pristine links reproduced the original ROM's SHA-256 exactly.

The expanded current variant is implemented as well. Lunar Magic replaces logical PC `$20509`
with `JSL runtime; RTS` and `$20566` with `JSL runtime+$40`. The exact embedded runtime halves were
captured from `005C3650` and `005C3698`; together they form a tagged 128-byte payload whose marker
at `+$3C` is `4C 4D 10 01`. Its 16-bit entry count is at `+$10`, and the four long table operands
are at `+$17/+$27/+$4C/+$5E`. The table allocation is four consecutive `2*N` planes. The legacy
variant has an `0xFFFFFFFF` marker, an 8-bit count where zero means 256, and pointer operands at
`+$14/+$24/+$47/+$59`; it is decoded independently from the current representation.

Instruction-level recovery of `OverworldWarpLinkDialogProc` (`00539CD0`) proves control `$0066`
has exactly 258 semantic rows. Row 0 is `No Setting / No Return Path Found`; row 1 is `Create a
one-way link to this tile`; rows 2 through 257 address native records 0 through 255. On OK, every
choice except row 1 first clears records whose displayed source matches the selected tile. Rows 2+
then subtract two, write the selected tile's display and runtime coordinate encodings into that
record, and copy them only when the Layer 1 selection bit remains set. Rust models the complete
combo-index boundary as `OverworldWarpReturnChoice`, including rejection of row 258.

A retained isolated-Wine save now proves that behavior at the four-plane file boundary. Authentic
Layer 1 records `$003A/$00EB`, both raw type `$82`, open `Link Star and Pipe Tiles`. Replacing the
default combo rows 3/5 with 27/28 moves the selected pair into native records `$19/$1A`, clears the
prior owners `$01/$03`, and preserves the other 23 records exactly. Lunar Magic saves a
checksum-identified 1,049,088-byte ROM; Rust's detected loader exports the complete before/after
tables into the retained canonical `LMOWWR1` hexadecimal fixture. Selecting record `$0000` (type
`$00`) instead produces the original `Wrong type of tile!` rejection, and dismissing it plus a save
leaves the complete successful-ROM SHA-256 unchanged.

The Rust migration boundary upgrades that legacy variant explicitly. It requires the runtime and
four contiguous planes to be two exact, non-overlapping RATS allocations whose payload starts
match the hooks and decoded operands. Migration reclaims those blocks only in a staging image,
tries the reclaimed region before growing the ROM, installs the recovered current 128-byte
runtime, publishes both hooks and all four long operands, repairs the checksum, semantically
reopens as `CurrentPatch`, and commits one undo entry. Missing ownership or malformed layout leaves
every ROM byte unchanged.

The clean-room installer applies exact pristine hook preconditions, allocates the runtime and table
as separate RATS blocks, resolves both code hooks and all four table operands through LoROM address
mapping, repairs the checksum, and commits expansion as one history batch. Installed-current saves
require both hooks to agree, require the old planes to be one exact contiguous RATS allocation,
replace or relocate it failure-atomically, and republish the count and all pointers. A built CLI
test installs 30 links into the supplied pristine ROM, reopens and exports them, then grows the
same installation to 40 links and reopens it with a matching model and valid checksum.

## Native overworld submap settings

The seven Lunar Magic submap-setting structures are not instances of the portable 12-byte
`SubmapSettings` model. `EnsureOverworldLevelSettingsRuntime` scans seven consecutive 0x20-byte
records in `OverworldLayer3Settings_ARRAY_00b45fb0`, and the relevant dialogs edit those records
directly. In the generated `$6E00` expanded-settings allocation they occupy record slots
`$200..$206`; slot `$207` is the separate trailing special/default record.

Rust therefore preserves all sixteen words of every submap record in `ExpandedOverworldSettings`.
The bounded 236-byte `LMOWSET1` file contains a 12-byte header followed by exactly seven records.
Both the CLI and application shell export defaults from a pristine SMW US revision-0 ROM, install
the complete expanded-settings runtime when necessary, update an existing table as one transaction,
repair the checksum, and semantically reopen all seven records before publishing output.

## Residual uncertainty policy

The remaining uncertainty is field- and ABI-level rather than unidentified-function coverage. Several internal helpers pass values in EAX/EBX/ESI/EDI or use mixed register/stack conventions that are not representable faithfully by an ordinary C prototype in Ghidra. Those register inputs are recorded in function comments and must be verified at callers before implementing the equivalent Rust API. Likewise, fields whose meaning is not proved retain reserved/unknown names inside otherwise size-correct structures. This is intentional: a false type or invented field name is more dangerous to compatibility work than an explicit unknown.

### Expanded overworld message storage

Live read-only analysis of Lunar Magic 3.63 under Wine resolved the current multi-bank overworld
message backend. The active SMW US revision-0 descriptor stores logical runtime offset `$1BD90` in
entry `$14B` and hook offset `$1C080` in entry `$14C`. `InstallExpandedOverworldNameRenderer`
copies a `$110`-byte runtime from executable address `005C3CB8`, relocates eight mapper-dependent
IRAM operands, and changes the hook to `JSL $03BD90 / JMP $B250`. The runtime ends in marker
`4C 4D 10 01` (`LM`, version `$0110`).

`CommitOverworldMessageBoxText` selects this representation when the modeled level-name count is
at least 97. It allocates a `6N`-byte pointer table for `N` levels—two three-byte message pointers
per level—and publishes its base and base-plus-one into runtime operands `$49` and `$4F`.
Messages are serialized in groups of at most `$C0` records. Each group is a separate RATS-owned
pool; trailing blank glyph `$1F` bytes are removed, short strings receive terminator `$FE`, and all
empty records in a group share one `$FE` byte. The pointer table contains absolute 24-bit SNES
addresses for every record. The editor workspace remains exactly 512 records of 144 bytes
(eight rows by eighteen glyphs); loading stops at `$FE` and pads the remainder with `$1F`.

The Rust profile now constructs this pointer-table/pool graph transactionally for even message
counts 194 through 512, verifies the retained executable resource, and semantically reopens every
message through the installed runtime operands. Detection requires the exact hook shape, fixed
runtime target, `$0110` marker, adjacent pointer operands, a valid RATS table, an even bounded
entry count, valid SNES mappings, exact RATS ownership for every 192-record pool, and either a
terminator or a complete 144-byte record. Installed updates replace/grow every pool and the pointer
table, republish both runtime operands, repair the checksum, and commit as one undoable revision.

The pristine side is now independently modeled instead of being treated as an install-only input.
SMW US stores the 23 selector bytes at logical PC `$2A590`, 25 little-endian offsets at `$2A5A7`,
and the source text blob at `$2A5D9`. The original `$05B1A5` selection loop scans entries `$16`
through `$01`, matches the high selector bit against message trigger 1/2, and falls back to entry
zero. Its renderer consumes at most eighteen glyphs per row; a glyph high bit retains the low
seven-bit tile and blanks the remaining row. The Rust decoder executes those rules for all 97×2
logical slots, and a pristine-to-expanded installation test proves semantic equality plus exact
undo to the original ROM.

### Native overworld event-reveal tables

Live Wine inspection of Lunar Magic 3.63 resolved the two main event-reveal planes independently
from the compressed event-tilemap runtime family. In the active headered SMW US revision-0
descriptor, entries `$103` and `$109` contain file offsets `$25C74` and `$25C84`; removing the
copier header gives logical long-address operands at `$25A74` and `$25A84`. Their pristine
operands resolve to logical table bases `$2585D` and `$25A1D`.

`LoadOverworldEventRevealTables` uses a RATS payload length when either plane points into expanded
storage and otherwise reads exactly `$E0` bytes, or 112 words. Its editor buffers accept at most
`$1FE` bytes, or 255 entries. Source words are little-endian and values above `$07FF` normalize to
zero; destination words are big-endian. Ordinary expanded saves use two equal, independently
RATS-owned planes. A real pristine `-TransferOverworld` operation proves a third representation:
only the source operand is relocated to a `$F0`-byte RATS payload. The loader therefore
materializes 120 source/destination pairs, with eight appended zero source words, while continuing
to read the destination plane through its fixed operand for the RATS-derived length.

The Rust boundary detects pristine fixed planes, this exact ownership-backed tagged-source/fixed-
destination behavior, or two non-overlapping RATS allocations with equal even lengths. Saves
validate the semantic model, allocate or replace both planes transactionally, publish both long
operands, repair the checksum, and require semantic reopen. The bounded `LMOWEVT1` interchange
preserves one to 255 source/destination pairs, and `LMOBS1` exposes every pair independently.
Profile, CLI, and application tests cover pristine decoding, 112-to-200 installation, growth to
255 entries, the Wine-derived 112-to-120 hybrid load, undo to the byte-exact original ROM,
built-process import/export/oracle replay, and save-as reopen.

The recovered editor movement boundary stores each destination as twice a seam-aware main-map
tile index. A selected ordinary reveal owns a 6x6 footprint; a requested drag is constrained by
searching X toward zero and then Y toward zero until every selected footprint fits. The Rust
cross-variant regression applies that semantic movement to both the pristine fixed table and the
retained Lunar Magic transferred-source hybrid, saves each as two owned expanded planes, moves and
republishes the expanded owner again, and semantically reopens both revisions. Headerless and real
512-byte-headered containers produce identical logical ROMs, preserve their exact physical prefix,
repair the checksum, undo to both exact predecessors, and redo both edits byte-for-byte.

### Overworld event-number mapping

`LoadOverworldEventNumberMap` at `004BA220` and `CommitOverworldEventNumberMap` at `004BA3B0`
resolve a separate byte-valued map from overworld event IDs to their effective event numbers.
The pristine US revision-0 path clears 256 bytes and expands eight source/value pairs from logical
`$001EE0`; a probe at `$0257F9` selects an alternate fixed 96-byte legacy representation.

Lunar Magic 3.63 installs a retained 32-byte version-1.10 runtime at logical `$02DD80` and changes
the four bytes at `$001F19` to `JSL $05DD80`. The runtime's table long operand is at `+7`, its
mapper-relocated IRAM word is at `+11`, and its final marker is `4C 4D 10 01`. Maps through event
`$5F` use the 96-byte region at `$02DDA0`; extended maps use the complete 256-byte region at
`$01BE80`. The loader also accepts an exact RATS payload through the runtime operand, bounded to
256 bytes.

Rust detects legacy pairs, legacy fixed storage, both current fixed regions, and exact tagged
relocations. Installation checks the original hook and unused runtime reservation, writes the
recovered runtime and selected table, relocates both long addresses, repairs the checksum, and
semantically reopens a staged image before committing one undoable edit. `LMOWMAP1` preserves the
meaningful 96-through-256-byte prefix. CLI and application workflows cover pristine export,
extended installation, undo/redo, create-new save, and built-process reopen. The real
`-TransferOverworld` fixture confirms that Lunar Magic spells the hook and table operand through
the low-bank LoROM mirror (`$05...`/`$03...`) rather than the byte-equivalent canonical `$85...`
or `$83...`; detection and emission now retain that choice exactly. The transferred 96-entry map
is semantically equal to the pristine legacy-pair map.

### Special-event reveal bundle

The later half of `LoadOverworldEventRevealTables` reads exactly 24 special-event source words,
24 destination words, and 24 direction bytes. Active descriptor entries `$10C/$10D/$10E` identify
logical long operands `$02669C/$026EC9/$02667C`, whose pristine targets are
`$0265B6/$026587/$0265D6`. Sources are little-endian and normalize above `$07FF`; destinations
are big-endian; directions are lossless bytes.

`CommitOverworldEventRevealTables` always relocates these three planes when saving and coordinates
two shared bank-safe pointer runtimes. The first is a 64-byte `LM 00 01` payload with entry points
at `+0/+0x20`, a source-table operand at `+0x26`, and a self-call at `+0x2E`. It installs `JSL`
hooks at `$026DDD/$026EC3`, byte repairs at `$026EDD/$026EE1`, a 20-byte inline fragment at
`$026F27`, and the fixed 16-byte helper at `$037540`. The second is a 48-byte `LM 00 01` payload
with entry points at `+0/+0x10` and hooks at `$0266C5/$026EF1`.

Rust installs the three table planes and both runtimes as five mutually relocatable RATS payloads
under one exact-precondition plan. Detection requires all five exact ownership descriptors, both
markers and runtime bodies, every hook/addend, both runtime source owners, inline/helper bytes, and
the fixed 48/48/24-byte shapes. Updates copy-on-write all planes, republish both source pointers,
repair the checksum, and semantically reopen. `LMOWSPC1`, CLI, and application workflows cover
pristine export, installation, installed updates, undo/redo, create-new saving, and process-level
reopen. Wine-derived detection proves all five owners and confirms that every emitted long pointer
uses Lunar Magic's low-bank LoROM mirror. The 24 transferred records are semantically unchanged,
and an edit/save/reopen/undo test restores the byte-exact Wine output.

### Compressed overworld event-tilemap buffers

`LoadOverworldEventTilemapBuffers` at `004B9930` and
`CommitOverworldEventTilemapBuffers` at `004B9C60` own two further game-visible event buffers.
The primary stream is exactly `$1000` planar bytes: `$800` event-index bytes followed by `$800`
auxiliary bytes. It is not a little-endian word array. The secondary editor buffer contains
`$800` words, but Lunar Magic persists only each word's high byte as a separate `$800`-byte
stream and combines those bytes with an already-loaded low-byte base plane.

Both streams use the ROM-selected LC_LZ2 or LZ3 codec and are independently RATS-owned. Live
descriptor inspection recovered the US revision-0 `A2` loader markers at logical
`$0257F9/$025818`. The primary split pointer owns its low word at `$025803` and bank byte at
`$025808`; the secondary pointer uses `$025822/$025827`. The `$200` difference from the live
descriptor is the oracle ROM's copier header. The pristine fallback scans 2,048 base
words, selects tile values `$56..$81`, assigns sequential event indexes, and sources auxiliary
bytes from the legacy table.

Rust now exposes the exact owned `EventTilemapBuffers` model and `LMOWTIL1` interchange boundary:
`$1000` primary bytes plus `$800` secondary high bytes. An explicit overlay operation combines the
latter with an independently loaded base word plane without claiming ownership of its low bytes.
Bounded LZ2/LZ3 decoding requires exact marker, fixed-runtime, hook, opcode, and RATS evidence.

The pristine installation plan reproduces Lunar Magic 3.63's 64-byte primary loader at `$0257F9`,
32-byte index helper at `$02DCD0` with hook `$02D8B1`, 48-byte reveal helper at `$01BA10` with
hook `$020F8A` and opcode repair `$021002`, and 160-byte state helper at `$01BA50` with hook
`$021199`. It allocates both compressed streams, publishes four split operands, repairs the
checksum, and semantically reopens the result as one undoable transaction. Dynamic comparison
corrected three hook bank bytes and two fixed loader call-bank bytes to Lunar Magic's exact
low-bank encodings. The pristine transfer materializes 92 event-index bytes and 74 auxiliary
bytes into LZ2-owned streams while leaving the 2,048-byte secondary-high plane zero. CLI and
application workflows install, update, export, undo/redo, and Save As through `LMOWTIL1`; the
combined transfer observer binds all four event domains independently of compression and RATS
placement.

The broader `smw-overworld-transfer-full-observe` boundary reuses the same Wine transition to
qualify thirteen native domains at once. In addition to the transferred Map16 definitions and
normalized acts-like values plus the four event domains, exact decoded observations cover special
paths, warp links, direct level names, player starts, the seven special expanded-settings records,
overworld messages, and boss-sequence messages. The pristine transfer
retains 14 path links, two starts, seven settings, 194 messages, and seven boss messages while
materializing 54 warp links from 27 stock records and 96 direct names from 93 stock names.

### Overworld boss-sequence messages

`LoadOverworldBossSequenceText` at `004BD740`, `CommitOverworldBossSequenceText` at `004BD840`,
and `ReleaseOverworldBossSequenceText` at `004BD990` own seven messages of eight 24-glyph rows.
The SMW US revision-0 table begins at logical `$04F1` and contains 56 contiguous 24-bit pointers.
Each pointed record has an exact 53-byte native form: a big-endian tilemap destination
`$5344 - row_in_message * $20`, the bytes `$00,$2F`, 24 interleaved glyph/`$39` pairs, and an
`$FF` terminator. The complete native payload is therefore `$B98` bytes.

The loader also accepts the stock legacy arrangement where rows are independently addressed.
Lunar Magic's commit path always allocates one `$B98` RATS payload and republishes all 56 pointers
at 53-byte strides; release recognizes that combined owner as well as legacy per-row owners. The
Rust implementation mirrors both detection forms, rejects partial or inconsistent combined
ownership, commits all pointers transactionally, repairs the checksum, and performs a semantic
reopen. The bounded `LMOWBOS1` file preserves the 1,344 logical glyph bytes independently of ROM
allocation, while the oracle exposes aggregate and independently addressable per-message hashes.

### Title/overworld layer and credits tilemaps

`CommitExpandedOverworldLayerTilemap` at `004C06E0` serializes the shared overworld-style Layer
tilemap used by the title-screen path. Each plane is exactly `$740` bytes: 29 rows of 32
little-endian tile words. The primary record begins `50 00 07 3F`; a nonblank secondary record
begins `54 00 07 3F`; `$80` terminates the stream. Lunar Magic omits the secondary record when
every tile number masked by `$03FF` equals `$00FC`. Rust preserves both materialized planes in
`ExpandedLayerTilemap`, reproduces the optional native framing, and exposes the allocation-neutral
`LMOWLYR1` boundary without conflating an omitted blank plane with absent model state.

Dynamic inspection of the running Wine process resolves active descriptor entry `$73` to headered
file offset `$06D3`, hence logical pointer operand `$0004D3`. The pristine operand is
`75 B3 05`, the low-bank LoROM address of logical `$02B375`. Initialization first fills the shared
scratch map with `$38FC`, executes the general graphics-remap stream, and copies the `$5000` and
`$5400` destination planes into the editor model. Saving instead emits the canonical one- or
two-plane literal form above, tags it with RATS, and republishes the low-bank pointer.

The Rust detected loader supports both forms and rejects any foreign unowned redirection. Its
transactional saver installs from pristine storage or erases and replaces only the exact current
RATS owner, repairs the checksum, and performs a semantic reopen. The
`smw-title-tilemap-export/import` process workflow is covered across pristine installation,
one-plane-to-two-plane update, allocation-independent oracle comparison, and input-ROM
immutability; the application command exposes the same undoable operation.

#### Main overworld Layer 1/2 runtime boundary

The `$740`-byte records above are not the main playable overworld map. They belong to the shared
title/overworld-style scratch tilemap path and must not be used as evidence that an edited main map
is consumed by SMW gameplay.

The main overworld editor entry point `InitializeOverworldEditorModel` at `00544570` initializes a
`$4000`-byte working plane at `00D94D98` and a separate `$9000`-byte working region at `00D98D98`,
then calls `LoadAllOverworldEditorData` at `004BF360`. The latter opens the ROM and dispatches the
Map16, event, ExAnimation, metadata, name, message, sprite, and expansion-state loaders. Its paired
`SaveAllOverworldEditorData` at `004BF550` validates the aggregate, releases each proven owner, and
commits those same subsystems transactionally. `SaveActiveOverworldEditorDataSet` at `00544800`
only selects the active editor backing set; its name does not establish a standalone ROM format.

`LoadInterleavedByteTableStreams` (`004B5CF0`) and `CommitInterleavedByteTableStreams`
(`004B5E00`) split the `$4000`-byte Layer 2 table into even and odd `$2000`-byte streams, compress
them independently, allocate them together, and publish two packed pointers. The call-site
assembly removes the prior ambiguity: `LoadAllOverworldEditorData` preserves its EAX argument in
EDI before calling the loader, and `InitializeOverworldEditorModel` supplies `00D94D98`; the save
entry passes that same address back to the paired commit routine. On SMW US revision 0 the
descriptor fields resolve to the game loader operands at logical offsets `$025C72`, `$025C79`, and
`$025C8D`. They initially address the two LC_RLE2 streams at `$022533` and `$02402B`, which the
game's `$04DC6A` loader materializes at WRAM `$7F4000-$7F7FFF`. Lunar Magic's authentic transfer
oracle relocates the pair together under one exact RATS owner while retaining identical decoded
tile words.

Rust now exposes this boundary as `load_smw_us_v1_main_overworld_layer2` and
`save_smw_us_v1_main_overworld_layer2`. The typed loader accepts only the exact pristine pair or a
single exact RATS owner, validates the encoded plane boundary and complete owner extent, and
materializes exactly 128x64 little-endian tile words. The canvas codec places the `$7F4000` main
map and `$7F6000` submap sheet side by side, and converts each plane's four 32x32 SNES screen blocks
to visual row-major order. Its exact inverse preserves runtime byte order on save. The saver rejects every other shape, keeps
both streams in one LoROM bank, updates both runtime operands in one transaction, repairs the SNES
checksum, and semantically reopens the edited cell. This authenticates playable Layer 2 storage;
Layer 1 and an emulator trace that observes the edited Layer 2 cell remain separate gates.

The native egui overworld window now has a profile-free SMW-US fallback backed by this controller.
It loads the authentic 128x64 Layer 2 map, vanilla overworld graphics slots `$1C-$1F`, and the
shared working palette. Each runtime word is decoded directly as one packed SNES 8x8 tilemap entry
(ten-bit tile number, palette, priority, and flips); it is not incorrectly expanded as Map16. The
canvas renders Layer 2 alone instead of fabricating unauthenticated Layer 1 data, and the brush,
rectangle, flood-fill, visual 8x8 picker, revision-bound commit, semantic reopen, and application
Undo paths operate directly on the gameplay-consumed streams.
The same profile-free window now loads the engine-detected native path-link table and exposes every
source endpoint, destination endpoint, submap byte, and engine target coordinate beside the map.
Route commits use the existing revision-checked application command, semantic reopen, checksum,
and Undo path. Terrain and route batches are deliberately staged one at a time so committing one
domain cannot silently discard an uncommitted edit in the other.
Selected route endpoints are drawn over the matching runtime plane, and dedicated source and
destination tools snap canvas clicks to the runtime's eight-pixel coordinate grid. The nonzero
mapping is now proved from the SMW runtime rather than inferred from the editor image:
`InitializeOverworldTilemaps` at `$04D6E9` uploads one shared `$7F6000-$7F7FFF` submap sheet;
`CalculateOverworldPlayerPosition` at `$049885` selects that shared submap plane whenever the
player's submap byte is nonzero; and `HandleOverworldPathExits` at `$049A24` compares the stored
X/Y words directly before separately publishing the destination submap byte. The six nonzero IDs
therefore share sheet coordinates and distinguish camera, palette, and gameplay state rather than
selecting six independent tilemap planes. The native overlay places IDs `$01-$06` on the right
512x512 sheet, applies the SNES background's nine-bit coordinate wrap (including vanilla endpoint
X/Y `$0200`), and retains the form's explicit nonzero ID when a right-sheet click sets an endpoint.
Left-plane clicks select main map `$00`; a right-sheet click with `$00` or an unsupported ID is
rejected instead of guessing which submap the user intended.

The ignored `native_main_overworld_layer2_paint_survives_snes9x_initialization` integration gate
drives that same application controller over a four-cell paint, commits the ROM, reopens the entire
128x64 gameplay layer byte-for-byte, verifies the repaired SNES checksum, and keeps the generated
ROM alive in the platform-discovered Snes9x executable for eight seconds. Its child guard kills and
reaps the emulator on success, failure, or panic. This proves editor-to-emulator initialization for
the authentic runtime storage, but not yet that gameplay navigated to and rendered the four cells.
SMW's fixed `OwExits` source and destination records physically store the Y word before the X
word. The native path-link codec now transposes only at that engine boundary, so editor,
automation, canvas, and runtime-evidence coordinates remain semantic X/Y while byte-for-byte
re-encoding preserves the original planes. A focused pristine-ROM oracle requires the first raw
`0140,0028,00` record to decode as X `$0028`, Y `$0140`, not the previously transposed view.

The companion `native_overworld_path_link_edit_is_traversed_in_snes9x` gate changes a
destination endpoint and matching engine target in the fixed gameplay path table through the same
revision-checked application boundary, semantically reopens the detected table, and verifies the
checksum. It no longer accepts an eight-second idle boot as gameplay evidence. An explicitly
configured platform driver must traverse the named source route in Snes9x and publish a tagged
snapshot plus PNG. The shared bounded snapshot decoder then requires overworld game mode `$0E` and
the exact edited destination in Mario's submap, position, and grid-position WRAM fields; the PNG
must be a decoded, nonblank Snes9x-sized game image. The supplied macOS/Linux libretro adapter boots
the exact generated ROM from reset, reaches the overworld through real controller input, restores
one baseline for each bounded adjacent source approach, and leaves movement, exit lookup,
destination assignment, and submap transition to SMW. An official Snes9x 1.63 arm64 core produced
an authentic tagged state plus 256×224 frame and passed the complete ignored gate in 1.58 seconds.
Windows and independent Linux release-host execution remain open.

Accordingly, `CompleteOverworldRomLayout` remains a profile-described editor/container boundary.
The ignored Snes9x complete-overworld smoke gate proves allocation, transaction, checksum,
reopening, and emulator initialization only; its extension pointer tables are deliberately outside
the stock gameplay call graph. Do not cite that gate as proof of rendered Layer 1/2 behavior. The
next runtime milestone is complete only when a typed SMW-US locator rejects unknown or partial
installations, an edited cell semantically reopens through that locator, and Snes9x reaches the map
code that reads the replacement data.

### Title-screen recording and playback

`ExtractTitleScreenRecordingFromSramBuffer` at `0048F340` reads a 128-KiB SRAM image. The movement
payload begins at SRAM `+$10000`; its encoded length-minus-four is little-endian at
`+$1FFF8`, the marker word at `+$1FFFC` must be `$0042`, the total length cannot exceed `$8000`,
and the final byte must be `$FF`. `WriteZsnesSavestateWithTitleRecording` at `0048FC90` places
that SRAM image after a zero-filled `$C13`-byte `ZSNES Save State File V143` prefix.

The Snes9x path accepts `#!snes9x:` or `#!s9xsnp:` snapshots, transparently inflates gzip input,
then walks 11-byte tagged-block headers from offset `$0E`. A six-digit decimal length occupies
header bytes `+4..+9`; the first `RAM` block of at least `$20000` bytes feeds the same SRAM
extractor.

Dynamic descriptor inspection resolves playback hook entry `+$574` to logical `$001C6F`,
the adjacent recorder hook entry `+$578` to `$0021DA`, and allocation search entry `+$C4` to
`$06ABF7`.
The pristine 17-byte hook begins `AE F4 1D`; installation replaces it with a `JSL` and fixed
continuation tail. Its RATS-owned `$60`-byte runtime begins `08 C2 20` and refers to the separately
owned movement payload at biased addresses `payload+2`, `payload-3`, and `payload-2`. A retained
playback-import oracle corrects an earlier misclassification: runtime bytes `+9..+10` are a fixed
zero initialization word, not a continuation operand. Lunar Magic allocates the recording before
the runtime in zero-filled expanded-ROM space at or above `$080000`, preserves the current ROM
size when space already exists, and preserves the stored checksum with its bounded compensation
run.

The Rust implementation validates every fixed hook/runtime byte, both owners, the fixed zero
initialization word, and agreement among all biased pointers. First installation allocates both blocks; updates
retain the proven runtime and reclaim only its proven recording owner. Allocation, pointer
publication, checksum repair, semantic reopen, and history commit are failure-atomic. `lm-title`
separates movement/container parsing from ROM mutation, while CLI and application workflows expose
native, ZSNES, and Snes9x import/export without platform APIs.

The original executable exposes `-ImportTitleMoves` and `-ExportTitleMoves` as authenticated batch
front ends to the same subsystem. A retained valid import produces the exact GUI/Rust ROM hash;
export recreates the complete minimal ZSNES V143 state byte-for-byte. Truncated input exits with
`Not a ZSNES Savestate!` before ROM mutation, while export from an uninstalled ROM exits with
`ASM code not detected!` and creates no output.

Against the authenticated 512 KiB vanilla ROM, batch import takes Lunar Magic's affirmative
not-enough-room path and expands to exactly 1 MiB. It changes internal ROM-size byte `$07FD7` to
`$0A`, initializes the fixed `$07F08E..$07F13F` metadata/padding and `$07FFE7..$07FFFF` feature
record, then allocates recording-before-runtime at `$080000`. Rust reproduces all physical bytes,
including the unchanged copier prefix and checksum-compensation distribution.
Replacing that installed payload with a 257-byte recording proves the reclaim fill is `$00` and
the compensation range is `$07EFA3..$07F09F` inclusive. The larger delta forces compensation into
the final metadata-padding bytes and distinguishes that bound from the shorter apparent range.

### Lunar Magic ROM attribution and feature metadata

`WriteLunarMagicRomMetadata` at `0047D3E0` is called by the real save path. Dynamic write tracing
under Wine resolves descriptor entry 1 to headered file offset `$7F2A0`, or logical `$07F0A0`,
where Lunar Magic 3.63 writes its exact `$A0`-byte attribution. Descriptor entry 0 resolves to
headered `$801E0`; the VRAM patch version is written at logical `$07FFE6`, followed at `$07FFE7`
by a 25-byte packed feature record.

The record contains a little-endian 32-bit feature mask, two packed configuration bytes, three
zero-or-`$42` markers, five little-endian 24-bit runtime pointers, and a final checksum-status low
nibble whose high nibble is cleared. Independent Wine fixtures produced by level 000, level 105,
palette, and ExAnimation saves retain the same framing while changing operation-dependent runtime
pointers. Rust models the attribution and record losslessly, decodes the proven fields, detects
pristine all-`$FF` storage versus a complete installed record, and rejects partial installations.
Writing all three regions plus the SNES checksum is staged, semantically reopened, and committed as
one undoable project transaction. The `LMROMMD1` file and allocation-independent oracle observation
provide a stable differential-testing boundary across real Lunar Magic saves.

### Expanded secondary-exit planes

`ApplyDialogToSelectedSecondaryExit` at `004112B0`, `LoadAllSecondaryExitTables` at `00473D30`,
and `SaveAllSecondaryExitTables` at `004742F0` prove the six-plane order used by ROM persistence:
destination low, position/method, packed screen/Y, destination high and flags, packed X/overworld,
and additional flags. This corrects an earlier Rust codec whose self-round-trip tests had hidden a
plane-1-through-3 permutation. A known-byte semantic test now fixes the native order.

For SMW US revision 0, the first four fixed planes begin at logical `$02F800`, `$02FA00`,
`$02FC00`, and `$02FE00`. Lunar Magic installs three seven-byte readers at `$06E190`; their
24-bit operands are at `+1`, `+8`, and `+15`. Three five-byte readers at `$02DC80` have operands
at `+1`, `+6`, and `+11`. Real LM 3.63 level-save fixtures use the compact form: the four vanilla
planes remain fixed, while the X/overworld and additional planes are separate RATS allocations of
one common trimmed length. The level-000 fixture trims them to `$1FE` bytes.

Rust now detects pristine and installed storage, validates every reader opcode and pointer,
requires exact ownership of variable planes, supports the compact and all-tagged layouts, and
materializes the complete 8,192-entry table. Installed updates select the same fixed-prefix versus
all-tagged threshold at `$200`, replace all variable owners transactionally, publish low-bank
LoROM pointers, repair the checksum, and reopen semantically. `LMSEXIT1` and its oracle hashes are
allocation-independent. Pristine installation is intentionally not synthesized from these reader
fragments alone: Lunar Magic coordinates it with the shared Lfix3 compatibility/runtime network.

The shared fragments are recovered without treating their embedded operands uniformly.
`InstallSecondaryExitBaseHooks` copies a `$30`-byte support routine to logical `$02DC50`.
The marker-bearing `$20`-byte first reader at `$06E190` addresses the four fixed planes, while the
`$50`-byte second reader at `$02DC80` contains one fixed-plane address and two independently
relocated RATS-plane addresses. A separate `$20`-byte index helper resides at `$06E1B0`. Rust
builds both readers as typed fragments and patches only the two proven ROM operands. Tests compare
the generated bytes against four independently relocated LM 3.63 Wine fixtures.

`InstallLfix3Runtime` preprocesses and allocates a shared `$510`-byte runtime rather than copying
an already-linked blob. The recovered LoROM transformation changes two mapper-sensitive absolute
loads and four branch operands, then applies 107 self relocations: two low-bank 24-bit references
and 105 low-word references covering code and dense dispatch tables. Two dispatch entries use
descriptor-selected aliases (`+$1C3` and `+$1D7`) instead of the literal pre-relocation words in the
embedded body. `lm-profile` now represents that transformation as a checked `PatchPayload`; a test
extracts the pristine template from the supplied LM 3.63 PE image, places it through the ordinary
Rust RATS allocator, and requires exact equality with the real `$080031..$080540` Wine-produced
runtime.

The Lfix3 core installation plan is now recovered as well. It installs the fixed `$20`-byte helper
at `$02DD00`, the `$50`-byte table helper at `$02DD30`, zero-initializes `$02DE00` and `$037C00`,
fills `$037E00` with the recovered `$1A` default, and applies the seven runtime entry hooks at
`$02DA17`, `$001708`, `$007871`, `$00777B`, `$00779D`, `$02BCA5`, and `$006966`. The fixed
entry hooks at `$0026CC`, `$02D97D`, and `$0052B2` are also identity-checked. Rust represents all
of these as one `RelocatablePatchPlan`, so allocation, hook validation, table initialization,
checksum repair, and undo are failure-atomic. Tests assert every generated hook operand rather than
only checking that installation returned success.

The final SMW-US-v1 composition adds the `$C0` extended runtime at `$01BCE0`, the `$1F`
compatibility helper at `$01BB00`, the base helper at `$02DC50`, readers at `$02DC80` and
`$06E190`, and the index helper at `$06E1B0`. It applies the recovered base, extended,
compatibility, shared-state, and three reader-call hooks under exact pristine preconditions. The
Rust plan allocates the Lfix3 runtime and either two or six equally trimmed plane payloads, fixes
all reader operands after placement, initializes the shared tables, repairs the checksum, and
commits once. Pristine compact and all-tagged semantic-reopen tests, late-hook rollback, application
undo, and a built CLI process test now cover the complete installation boundary.

The adjacent user-requested Sprite 19 ASM fix is recovered independently from hidden command
`$26AC`, `PromptAndInstallSprite19AsmFix`, and `InstallSprite19AsmFixRuntime`. A matched pristine
SMW-US-v1 transaction under Lunar Magic 3.63 replaces the six-byte hook at logical `$00E762`,
installs the fixed `$20`-byte helper at `$01BCA0`, and removes the three-byte branch at `$0020A0`.
The helper is shared with normal Lunar Magic level installation, so Rust distinguishes pristine,
authenticated shared-helper-only, and fully installed states: the shared state requires only the
final branch removal. Partial or modified forms reject rather than being silently normalized, and
the native, application, and CLI routes all authenticate the completed byte shape after applying
one checksum-repaired transaction.

`DetectLfix3RuntimeGeneration` classifies the migration family in priority order. The marker at
logical `$02DD7C` contains `LM` plus version `$0111` for generation 3; the executable's
`CMP $0111 / SBB / ADD` sequence accepts versions greater than or equal to `$0111`, despite the
older reverse-engineering label suggesting the opposite. Without that marker, a JSL at descriptor
entry `$263` (logical `$02DA17`) selects generation 2, and a JSL at entry `$119` (logical
`$02D7CE`) selects generation 1. Generation 2 legitimately retains the generation-1 hook. Rust now
authenticates generation 1 from both that exact JSL and its complete fixed `$02DC50` helper,
authenticates generation 2 from its `$240` RATS owner and complete immutable runtime network, fully
authenticates generation 3, and prevents the pristine installer from overwriting either legacy
family. The exact generation-1 table conversion is also recovered: for each of 512 entries with
legacy flag `$20` clear, packed bit `$10` moves to bit zero of the new plane and is cleared in the
packed plane. The table-preserving installation transactions remain to be authenticated against
retained legacy fixtures for generation 1. Generation 2 is now complete: Rust reclaims its exactly
authenticated `$240` RATS owner in staging, installs and relocates the current `$510` runtime,
preserves all three live planes byte-for-byte, repairs the checksum, authenticates the result, and
undoes exactly as one transaction. Generation 1 now migrates as well: the legacy hook/helper pair
is authenticated, every later core/runtime destination must retain its exact pristine
precondition, the live packed plane is rebound transactionally and converted into the recovered
two-plane form, the third plane receives its `$1A` default, and the current runtime plus checksum
commit in one undo batch. Corrupt fixed or destination-table bytes reject before mutation, current
authentication proves the result, and exact undo restores the full legacy image.

`LoadLegacyCreditsTilemapRows` at `004C1280`,
`LoadExpandedCreditsTilemapRows` at `004C13B0`, and `SaveCreditsTilemapToRom` at `004C0DE0`
operate on a 256×32 word editor tilemap. A row is trimmed to its first and last nonblank word,
encoded as `first_column, byte_count_minus_one, little_endian_words`; `$FF` represents an empty
row. Identical complete rows reuse one 16-bit record offset. The pristine SMW US revision-0 ROM
contains 202 offsets at logical `$061D18` and a `$751`-byte record region beginning `$0615C7`;
the remaining 54 editor rows materialize as `$38FC`.

Rust now decodes both the 202-row legacy and 256-row expanded logical shapes, canonically trims and
deduplicates rows, and exposes the exact 16,384-byte logical tilemap through `LMCREDT1`. The
capacity-preserving legacy writer remains available as a strict lower-level boundary.

The full Lunar Magic expanded installer is implemented as well. It moves the 256-word offset table
to logical `$061CAC`, changes the table low word immediately before `$061EEB` from `$9D18` to
`$9CAC`, and installs the recovered 96-byte runtime beginning `9B AA BF`. Its two long record
operands are at runtime offsets `+$03/+$30` and must agree on one exact RATS-owned payload. Updates
erase only that proven owner in a staging image, reallocate the canonical record stream, republish
both operands and all offsets, repair the checksum, and semantically reopen before one undoable
commit. CLI and application imports transparently migrate pristine storage and update installed
storage without orphaning the previous owned block.

Exact UI wording and some vanilla object/sprite identities are lower-confidence where no resource string or dispatch metadata proves them. The reimplementation should depend on recovered encoded behavior, geometry, table slot, and observable output—not on those tentative human-facing identities.

## Native custom overworld sprite records

`LoadCustomOverworldSpriteRecords` (`004BDE10`) and
`CommitCustomOverworldSpriteRecords` (`004BE670`) use one RATS-owned,
offset-addressed stream for custom sprites on seven overworld maps. The stream
starts with seven little-endian 16-bit offsets. Each selected list contains at
most 24 records and ends with a zero word. A record's total byte length comes
from the 128-entry sprite-size table loaded by
`LoadOverworldSpriteRecordSizeTable` (`004BDB10`).

The three-byte little-endian record prefix packs a 7-bit sprite ID, six-bit X
and Y coordinates in eight-pixel units, and a five-bit screen coordinate in
eight-pixel units. Remaining bytes are ID-specific extension data. Lunar Magic
sets prefix bit 7 when an otherwise-zero low word would collide with the list
terminator. Empty maps may alias the preceding terminator. Rust reproduces
these details in `NativeCustomOverworldSpriteTable`, while its oracle compares
the decoded ID, coordinates, screen, and extension bytes rather than offsets
or allocation location.

The recovered stream now has a revision-bound application transaction rather than only a codec
and project API. `NativeCustomOverworldSpriteController` loads the exact RATS owner from an
overworld snapshot, stages ordered insert/replace/remove/move-before edits across all seven maps,
canonically encodes and reopens the complete stream, then publishes one checksum-repaired ROM
mutation using the proven previous owner. A late invalid ID, extension width, coordinate, screen,
map count, record index, or 25th per-map placement cannot publish an earlier edit from the same
batch. The codec also rejects IDs `$80+` before indexing the 128-entry size table.

The active SMW-US descriptor values are now encoded as profile routing rather than caller-supplied
addresses. Descriptor field `+$114` is physical `$077750` (All-Stars `$277780`), making its
`+$0D` stream operand logical `$07755D`; field `+$BFC` is physical `$06E38C` (All-Stars
`$16638C`), making its headerless size-table operand `$06E18C`. ExLoROM selects the upper 4 MiB
body. The size-table loader's byte immediately after the operand is an installed `$42` marker;
without it the initialized table remains all fours. With it, the operand must target an exact RATS
payload of 128 bytes or the legacy 127-byte ID-1 tail, followed by low-nibble masking and clamping
to `3..15`.

The installed overworld editor consumes that resolved layout directly. Its native-sprite panel edits all
seven ordered lists and uses the currently selected canvas cell as an eight-pixel-aligned position.
Staged records are composed into the same native `.sscov`/`.s16ov` appearance pass as existing
sprites. Publication materializes the already prepared terrain/record/animation mutation first,
then allocates the native stream against that staged image and rebuilds one mutation relative to
the immutable source. Existing stream and size-table owners are protected during ordinary payload
planning; the authenticated stream owner becomes reclaimable only inside its own save. This avoids
free-space collisions while preserving one application revision and one Undo step across domains.
The installed editor's native-sprite canvas tool converts the 128×64 combined preview back into
the stream's local coordinates: map zero owns columns `0..63`, while maps one through six share
columns `64..127` and subtract 64 before the eight-pixel conversion. The explicit canvas-cursor
placement command replaces the selected record or appends when the insertion cursor is selected;
cross-plane positions cannot mutate data.

The original canvas interaction is now independently recovered from the live port-8089 program.
`HandleOverworldSpriteLeftButtonDown` (`0055BFB0`) converts the pointer to eight-pixel grid
coordinates. A plain hit selects the painter-topmost sprite (clearing the prior selection when the
hit was not already selected) and begins dragging the complete selected set; `MK_CONTROL` (`$08`)
instead toggles only the hit sprite. An empty plain press clears selection and begins a marquee,
while an empty Ctrl press retains the baseline selection and begins an additive marquee.
`ApplyOverworldSpriteSelectionRectangle` (`0055AFF0`) walks every render node intersecting the
rectangle and sets its owner in the 512-entry boolean selection array, so overlapping sprites are
all selected rather than only the hit-test winner. `HitTestOverworldSpriteAtGridPoint`
(`004D2250`) walks the 8,192-cell render grid and lets later matching nodes overwrite earlier ones,
which proves reverse-painter-order point selection.

`BeginDraggingSelectedOverworldSprites` (`0055B440`) captures the selected set and pointer origin.
`ConstrainOverworldSpriteMovePosition` (`0055B690`) derives a signed grid delta and validates every
selected owner. `FindValidOverworldSpriteMoveOffset` (`0055B4C0`) searches candidate offsets back
toward the origin; for custom IDs its compatibility condition is unconditional once the combined
64×128 grid position is valid, so the custom-stream editor needs common group-boundary constraint,
not collision rejection. `ApplyOverworldSpriteMoveAndRedraw` (`0055B860`) applies the common delta
and rebuilds the render grid. Movement may redraw during the gesture, but
`FinalizeOverworldSelectionInteraction` publishes one Undo record when the gesture ends. The Rust
installed editor mirrors this with exact rendered-footprint marquee selection, Ctrl/Command
toggle, retained multi-selection, common snapped boundary fallback, and one ordered controller
batch on release.

The original keyboard dispatcher closes the remaining shortcut ambiguity.
`HandleOverworldEditorKeyboardShortcut` (`005510D0`) sends Ctrl+A through command `$245D`, whose
sprite-mode branch calls `SelectAllOverworldSprites`; unmodified Delete sends `$245B`, whose
sprite-mode branch calls `DeleteSelectedUnsupportedOverworldSprites` (`0055A990`). Ctrl+C
(`$2455`) and Ctrl+V (`$2456`) explicitly return without action in sprite mode: those clipboard
commands are limited to Layer 2 and Layer 1 tiles. Right-button down (`WM_RBUTTONDOWN`, `$0204`)
instead routes sprite mode to `PasteCustomOverworldSprites` (`0055C350`). That calls
`DuplicateCustomOverworldSpritesAtPosition` (`0055C140`), which copies every selected custom
sprite in stable slot order, clears the old selection, selects the copies, constrains the group at
the pointer, and begins dragging it. The Rust editor follows that evidence with focused Ctrl/Command+A,
Delete, and a deferred right-drag duplicate batch; duplication and final positioning publish
together as one application revision rather than exposing a partial copy between pointer frames.

`EditCustomOverworldSpriteProperties` (`0055BE60`) proves the Alt variation is a separate action,
not modified duplication. In pointer mode it bounds the combined grid, hit-tests exactly one
painter-topmost custom sprite, copies that record into the property-dialog buffer, and opens the
modal. Accept writes the edited ID, vertical extent, and twelve-byte property buffer back to that
record and publishes one Undo entry; cancel returns without modifying the record. Because the
outer `PasteCustomOverworldSprites` condition skips duplication when that edit succeeds,
Alt-right-click never creates a copy. The Rust canvas routes the same modifier/hit combinations
through a dedicated property modal, blocks competing canvas/form/commit mutations while it is
open, applies one typed `Replace` only on acceptance, and retains the prior multi-selection.

## Per-slot `ExAnimation` options

`DecodeExAnimationSlotOptionFlags` (`004B3CB0`) and
`EncodeExAnimationSlotOptionFlags` (`004B3E80`) operate on seven packed bytes,
one for each level `ExAnimation` slot set. Bits 4–7 use inverted polarity:
the corresponding editor option is enabled when the stored bit is clear.
Encoding preserves each low nibble verbatim.

`CommitExAnimationSlotOptionFlags` (`004B4000`) writes the seven bytes as one
RATS allocation and patches the installed runtime operand. This allocation is
independent of the compact per-level and global record streams. Rust therefore
models it as `ExAnimationSlotOptionTable`, validates the exact seven-byte
shape, preserves low nibbles, relocates it transactionally, and observes all
35 semantic/preserved fields independently.

## Native `-TransferOverworld` Layer 2 and Map16 allocations

Tracing `SaveAllOverworldEditorData` (`004BF550`) through the live 3.63
decompilation resolves the first allocations in the pristine-US Wine oracle.
They precede the overworld-specific event/text allocations because the native
operation saves the playable Layer 2 base and Map16 support tables in the same transaction:

- Payload `0x80008`, length `0x2F28`: the 16 KiB playable overworld Layer 2 table. Lunar
  Magic splits even and odd bytes into two 8 KiB planes, sized-RLE encodes the
  planes back-to-back, and interleaves them on load. The fixture's planes
  consume 6514 and 5558 encoded bytes. Rust exposes this as
  `decode_interleaved_sized_rle_prefix`; the installed game loader operands point directly at
  these two streams.
- Payload `0x82F38`, length `0x0B44`: 2884 raw low bytes of the trimmed expanded
  Map16 acts-like table.
- Payload `0x83A84`, length `0x0604`: the corresponding compressed high-byte
  plane. `CommitExpandedMap16ActsLikeTable` (`004B63C0`) intentionally stores
  the planes as separate adjacent RATS owners.
- Payload `0x84090`, length `0x05CC`: 371 four-byte Map16 remap-range records
  flattened by `CommitMap16RemapRangeGroups` (`004B6980`).
- Payload `0x84664`, length `0x00A0`: current-format grouped-remap runtime. Its
  patched operands point to the four independently owned planes below.
- Payload `0x8470C`, length `0x00F2`: 121 little-endian byte offsets delimiting
  120 groups.
- Payload `0x84806`, length `0x002C`: flags for 44 grouped remap records.
- Payloads `0x8483A` and `0x8489A`, length `0x0058` each: the corresponding
  44 little-endian source and destination tile words.

This corrects the provisional hypothesis that all four large owners were Map16-only data. The
first is the playable Layer 2 base; the following owners are acts-like and remap metadata installed
by the top-level overworld save. `LoadMap16RemapRangeGroups` (`004B6750`) validates both words
of every four-byte pair against the 16K Map16 bound. The current-format
`LoadGroupedMap16RemapRecords` (`004B74E0`) reconstructs linked editor records
from the three parallel planes; flag bit 0 selects the 16K-to-16K form,
otherwise the destination is constrained to the stock 2K range. Rust models
both formats without the linked-list implementation detail and observes every
group and record independently of allocation placement.

## Native overworld Layer 3 settings

The combo and edit variants at `0041A120`, `0041A440`, `0041A6D0`, and
`0041A970` establish the exact 32-byte record used for each of seven overworld
maps. `LoadOverworldLayer3Tilemap` (`004C1A20`) consumes flag `$2000`;
`LoadOverworldLayer3Graphics` (`004C1B30`) consumes flag `$4000` and the four
graphics words at offsets `$18..$1F`.

The record layout is:

- `$00..$01`: feature flags, preserving all flags other than the two proven
  Layer 3 selectors during focused edits.
- `$02..$03`: 12-bit tilemap file index, two-bit size, and two-bit position.
- `$04..$13`: eight words transformed by
  `ConvertOverworldLayer3AnimationAddressLayout` (`00544860`).
- `$14..$17`: preservation-only until consumers establish their semantics.
- `$18..$1F`: four graphics-file words; the low 12 bits are the file index and
  the high nibble is preserved.

Rust represents the seven contiguous records as
`OverworldLayer3SettingsTable`, exposes only proven fields, retains the exact
224-byte encoding, and provides transactional direct-table I/O plus a semantic
oracle. Native persistence is expanded-settings slots `$200..$206` inside the
validated `0x6E00` RATS allocation; the SMW US revision profile derives the
direct offset from that allocation's table base and 32-byte stride. A retained
Wine-generated ROM test proves that the semantic view and the generic expanded
settings view produce identical records. The in-process address `00B45FB0` is
therefore used only as runtime-structure evidence, never as a ROM file offset.

`smw-overworld-layer3-settings-observe ROM OBSERVATION` emits the proven packed
fields and preservation regions independently of allocation location. The
application shell accepts the same semantic table as one revision-checked,
checksum-repaired, undoable replacement; pristine ROMs install the owning
expanded-settings runtime first.

## Pristine sprite-stream growth interoperability

Vanilla SMW stores all level sprite pointers as low words with one shared bank
byte. A growing stream therefore cannot be moved to an arbitrary free bank
without first installing a different pointer format. The pristine editor now
performs copy-on-write relocation inside that exact shared LoROM bank, writes a
RATS-owned canonical stream, updates only the selected low-word pointer, and
repairs the ROM checksum in the same transaction.

The ignored `lm-app` integration test `sprite_growth_wine` supplies independent
dynamic evidence for this path. It inserts a duplicate sprite into level
`$105`, commits through the public Rust controller, opens the resulting ROM in
Lunar Magic 3.63 under Wine, and invokes the documented `-ExportLevel`
interface. The exported MWL sprite section decodes to the exact expected
ordered `NativeSpriteStream`, including its header and inserted token. Lunar
Magic also leaves the edited ROM with a valid checksum. This proves
interoperability at the original application's loader/exporter boundary rather
than relying solely on a Rust encode/decode round trip.

The reciprocal direction is verified independently. `MwlDocumentController`
now decodes and replaces a typed `NativeSpriteStream` while retaining both
opaque MWL provenance words and every unrelated section. Its revisioned
transaction rejects stale revisions and malformed or noncanonical records
without changing history. The Wine oracle exports pristine level `$105`, adds
a duplicate sprite through that public Rust API, invokes Lunar Magic
`-ImportLevel`, then re-exports the installed level. The final sprite stream is
semantically identical to the Rust edit and the resulting ROM checksum is
valid. Together, the two directions prove both direct-ROM and MWL-based sprite
editing across the Lunar Magic 3.63 compatibility boundary.

The native sprite placement format has also been corrected from direct format
evidence. Its base record is `yyyyEESY / XXXXssss / NNNNNNNN`: Y is the upper
nibble plus bit 0 of byte one, extra bits are byte-one bits 2–3, the five-bit
screen is byte two's low nibble plus byte-one bit 1, X is byte two's upper
nibble, and byte three is the sprite number. The expanded `FF 00..7F` command
sets the upper seven Y-position bits; it is not a screen selector. The earlier
portable decoder incorrectly read the low nibbles as coordinates and treated
that command as a screen change, which displaced native GUI/render previews.
`NativeSpritePlacement` now follows the proven packing and carries a `u16`
minor axis for expanded Y positions.

`NativeSpriteRecordFields` provides lossless editing of all five base fields.
It reconstructs only the three-byte prefix, retains every custom extension
byte, and rechecks the four-table `(extra bits, sprite number)` record length
before commit. Thus an edit that would silently change a custom record's width
is rejected atomically. The MWL sprite panel exposes these fields and its list
shows decoded screen/X/Y/extra-bit placement. The strengthened Wine oracle
changes X and Y through this API; Lunar Magic 3.63 imports and re-exports the
exact resulting stream.

The pristine-SMW native ROM editor now consumes the same field API rather than
requiring hexadecimal record reconstruction. Its selected-record form exposes
sprite number, screen, X, low Y, and extra bits, disables semantic application
for expanded control commands, and sends the repacked record through the
existing revisioned `LevelController` transaction. Raw bytes remain available
for lossless inspection and advanced edits. The canvas now forwards the real
level-mode byte and recovered horizontal/vertical orientation to the standard
sprite dispatcher in addition to the placement's first byte. This matters for
the position- and mode-dependent generator handlers (`$E5`–`$EB`), which
previously always displayed the default horizontal mode-zero variant in the
Rust GUI despite the renderer already modeling the recovered branches.

Canvas sprite movement now reverses that same placement transform. A drag
started on a sprite records its stable token index; dropping converts the
horizontal or vertical canvas tile back into the five-bit screen, four-bit X,
and low five Y bits, preserves sprite number and extra bits, and applies the
semantic record edit transactionally. Drops outside the representable
32-by-512-tile native space are rejected without mutating the level stream.
The direct-ROM view now retains a fixed 12-pixel tile scale inside a two-axis
scroll area instead of shrinking or clipping long levels. Its major axis grows
through all 512 native tiles and its minor axis grows from 16 to 32 when sprite
placements use the second row; grid, artwork, selection, and drag hit testing
all consume the same orientation-aware rectangle.
The direct-ROM canvas now also exposes explicit one-shot object and sprite placement modes.
`insert_ordinary_object_at` adds an ordinary object at the clicked absolute screen, stably places
it after existing objects on that screen, regenerates minimal advance/jump transitions, and
preserves trailing opaque controls. Sprite placement rewrites a valid form record to the clicked
screen/X/Y through the recovered packed fields and applies the proven stable legacy screen sort.
Invalid command-zero objects, sprite controls, width mismatches, and off-canvas clicks fail before
the staged level changes.
The visual Add Object catalog covers every noncontrol ID `$01–$3F`. It resolves the selected
level's normal, castle, rope, underground, or ghost-house family through the exact pristine-ROM
handler map and renders a one-record minimum-parameter stream into an isolated 16×16 Map16 cache.
The supplied normal-family ROM fixture currently proves authenticated visible cells for 45 IDs;
the remaining valid commands are labeled rather than assigned speculative artwork. Selection
initializes the minimum parameter and arms the same absolute-screen insertion transaction.
Resolved OSC displays now supply a second active-variant Add Object catalog. It deduplicates
object/parameter selectors for the current normal, castle, rope, underground, or ghost-house
family; searches hexadecimal pairs and descriptions; and fits composite Map16 artwork using
built-in or external definitions. Native placement deliberately derives its 3–8-byte record shape
from `GetEncodedLevelObjectRecordLength` rules for the selected command, retaining required
extension bytes through coordinate placement. The OSC 2–15-byte compact/linear metadata field is
not treated as level-stream framing, matching the recovered loader boundary.
The corresponding Add Sprite workflow now presents all standard IDs `$00–$ED` in a bounded,
hex-filterable visual catalog. Each cell calls the authenticated standard-sprite dispatch table
with the current packed position byte, full native major/minor coordinates, level mode, and
orientation, fits its complete composite geometry into a preview cell, and labels empty/default
handlers explicitly. The full axes are derived exactly like stream placement (`screen * 16 + X`,
five-bit minor), while the handler's first byte is the two coordinate nibbles rather than the
serialized `yyyyEESY` byte containing extra/screen flags. This preserves parity/direction and
absolute-layout handler inputs. Selecting an entry
constructs the proven `yyyyEESY / XXXXssss / NNNNNNNN` record, retains the form's position and
extra bits, and arms the same transactional canvas placement path.
Both this catalog and the SSC atlas catalog now pass the current animated GFX33 texture alongside
the ordinary SP atlas, so subtile page bit `$0200` selects the same source before and after placement.
The standard catalog additionally applies the placed renderer's recovered `$E1/$1B8` and
`$90/$1C0-$1F3` half-opacity scopes. Both the catalog and placed renderer now require a standard
preview source before applying them, preventing same-numbered SSC definitions from inheriting a
built-in handler's translucency.
Placed SSC sprites now derive interactive bounds from resolved display-part offsets independently
of raster availability. Missing external graphics or palettes retain the unresolved marker but no
longer discard negative or extended composite geometry from selection and hit testing.
Resolved SSC selectors now supply the direct-ROM editor's actual four 256-entry native record-length
tables instead of being used only for artwork. The editor key includes a stable length-authority
signature, so attaching, removing, or changing the relevant SSC selectors forces a complete level
redecode. Conflicting length declarations for one sprite/extra-bit table fail before stream
framing. Semantic edits and canvas movement validate against that same table and preserve every
extension byte. A custom visual catalog deduplicates nonalternate default displays, searches IDs
and descriptions, resolves built-in or external Map16 preview definitions, and constructs an exact
declared-width record with deterministic zero extension bytes for one-shot placement.

`LoadSscCustomSpriteMetadata` (`00444E50`) and
`RenderM16SidecarObjectsToPixelBuffer` (`0044F6AE`) prove the global SSC remap semantics that were
previously parsed but not rendered in Rust. `$10000` ranges select a graphics base (with the
recovered mode biases `$2000`, `$0000`, `$0400`, and `$0900`) that is added to every Map16
subtile's ten-bit graphics index. `$20000` ranges select an external palette block for the
definition; absent entries use the normal sprite palette. The renderer now retains both values in
its public preview model, applies bases that fit the loaded 1,024-tile atlas, and refuses to
fabricate unavailable external pages or palettes. The native atlas itself now contains all eight
sprite palette rows, so the three palette bits and both flip bits in every subtile affect the
visible standard and custom preview.

`LoadExternalSpriteGraphicsAndPalette` (`0045BAE8`) completes the backing-file boundary. It reads
eight sibling `ExternalGraphics/ExSpriteGFX%02X.bin` files into fixed `$8000`-byte regions whose
global tile bases are `$2000`, `$2400`, `$2800`, `$2C00`, `$3000`, `$3400`, `$3800`, and `$3C00`.
It then prefers `ExSpritePalette00.mw3` and falls back to `ExSpritePalette00.pal`.
`LoadExternalPaletteFile` (`0045B1D0`) bounds those formats to `$8000` bytes of little-endian
SNES BGR555 words or `$C000` bytes of RGB triplets (1,024 rows of 16 colors). Rust now models that
boundary with atomic bounded decoders and a software SSC rasterizer that combines the global
graphics base, external palette base row, each subtile's palette bits, transparency, and flips.
The native SSC workflow searches the selected sidecar's nearest project ancestors for
`ExternalGraphics`, loads only present files on its bounded document worker, honors the recovered
palette preference, and publishes one asset revision. Custom catalog and level-canvas textures are
keyed by the complete remapped definition and discarded whenever that revision changes, preventing
art from a previously opened sidecar from leaking into a new project.
The raster boundary accepts graphics and color resolvers independently. The native editor retains
the indexed SP1–SP4 tiles and composed level palette alongside their display atlases, so SSC
external palettes work with ordinary sprite graphics and external graphics work without a
`$20000` override by using palette rows 8–15. A ROM, tileset, or palette reload clears the remapped
texture cache in addition to an SSC asset change.
Fresh inspection of `RenderM16SidecarObjectsToPixelBuffer` (`0044F6AE`) also corrects the global
source routing: the `$10000` mode-1 bias `$0000` is the foreground cache, mode 2 bias `$0400` is
the SP cache, mode 3 bias `$0900` is the Layer 3 cache, and mode 0 bias `$2000` is external sprite
graphics. The native renderer adds the ten-bit subtile index before reading its shared 64-byte
decoded-tile buffer. Its ordinary palette base is rows 0–7 only when the complete graphics base is
zero and rows 8–15 for every nonzero base. Rust now retains foreground, SP, and pristine Layer 3
indexed tiles as separate sources, subtracts `$400` only for SP lookup, and routes `$900`–`$CFF`
through an exact eight-slot 2bpp Layer 3 cache. The first four `$800`-byte slots load GFX28–2B and
the four `$7F` markers materialize as blank 128-tile slots, matching `LoadLayer3GraphicsSet` at
`00464750`. A validated installed `STAR` allocation supplies words 15→12 of the active level's
expanded record in that slot order; an absent allocation supplies the recovered pristine record.

The standard-sprite renderer now also covers every late native dispatch-table entry beyond the
ordinary picker boundary. Exact disassembly of `$004CAFB0–$004CB23E` proves that `$EF`, `$F2`,
`$F3`, `$F4`, and `$F5` are compatibility aliases of the `$E7`, `$EA`, `$EB`, `$EC`, and `$ED`
text previews, including their level-mode and placement-nibble branches. `$EE`, `$F0`, and `$F1`
retain the empty default entry. `$F6–$FF` all point to `$004C3A00`, which increments native
bookkeeping and delegates custom-display handling when configured but supplies no built-in
preview definition; the Rust renderer therefore returns no standard artwork for those IDs.

The profile-qualified ROM native-assets workflow now consumes the already recovered Layer 2
classification and I/O boundary instead of leaving it headless. `NativeLevelAssetsController`
loads an optional fifth payload from the profile's Layer 2 pointer table, stages either ordered
object edits or exact 1,024-word tilemap edits, and rejects storage-mode mismatches atomically.
Both copy-on-write and manifest-owned reclamation commits serialize Layer 1, sprites, Layer 2,
palette, and ExAnimation with the optional expanded-settings write and one checksum repair. The
native window exposes the corresponding conditional Layer 2 tab.
The standalone checksum-atomic writer is now reciprocally proven against Lunar Magic 3.63: a
level-105 legacy tilemap edit is allocated in a newly expanded LoROM bank, repointed through the
recovered `$02E600` table, and re-exported by Lunar Magic with the exact expected 2,048 decoded
bytes.

The first direct-ROM move oracle exposed one additional native invariant:
Lunar Magic stably sorts legacy sprite records by their decoded five-bit
screen after a cross-screen move, preserving the prior priority order among
records on the same screen. `sort_legacy_records_by_screen` now stages and
validates that operation, reports the selected record's new index, and rejects
expanded/control/short streams atomically. Canvas drag batches field
replacement and sorting in one `LevelController` edit. The strengthened Wine
test grows and relocates level `$105`'s stream, changes screen/X/Y, invokes the
stable ordering rule, and receives the exact same sprite stream from Lunar
Magic 3.63's subsequent MWL export.

The installed canvas now dispatches by sprite framing at the same interaction boundary. Legacy
records retain the stable screen sort above. Expanded drags call the upper-Y relocation model
directly, while expanded placement atomically inserts the record and relocates it from the active
shared state. The model removes redundant `$FF vv` transitions, emits only state changes,
preserves extension bytes, stably sorts by screen and then resolved upper-Y band, retains priority
within an identical screen/band pair, and returns the selected record's new token index across both
sorting and removed controls. This matches the comparator at `004CBFA0`: its first two keys are the
decoded screen byte and expanded upper-Y byte, followed by the original linked-list relation for
ties. When the vertical-level global at `$00E27909` is set, it first compares the high nibble of
the parser's orientation-swapped second record byte—equivalent to the original low four Y bits.
The live `lunar_magic_matches_vertical_expanded_sprite_ordering` oracle isolates that third key in
an authentic vertical installed level. A synthetic SMW-US ROM fixture commits,
semantically reopens, and undoes both installed-canvas operations.

The parser at `004CC185–004CC218` resolves the remaining `$FF 80..FD` range: values at most `$7F`
replace the active upper-Y byte, `$FF` escapes a record, `$FE` terminates, and every `$80..FD`
value advances the input by two bytes without changing state or allocating a sprite node. The live
`lunar_magic_strips_ignored_expanded_sprite_controls` oracle injects `$80` and `$FD` around a real
upper-Y transition; Lunar Magic strips both ignored pairs while preserving the transition and
complete record sequence. Semantic serializers must therefore remove these pairs, while raw
fixture codecs may retain them losslessly.

The framing discriminator is per stream rather than a property of the installed pointer-table
generation. `SerializeLevelSpriteList` (`004CC2B0`) clears sprite-header bit `$20` before emitting
the legacy one-byte `$FF` terminator. If it emits an upper-Y control, an escaped `$FF` record, or
another expanded token, it sets header bit `$20` and appends `$FE` after the terminator.
`ParseSerializedLevelSpriteStream` (`004CC130`) selects that same expanded grammar from the format
flag supplied from the serialized header. Consequently one Lunar Magic-installed per-level-bank
table can legitimately address both legacy and expanded streams; neither the table hook nor a
trial parse is a safe discriminator.

The same authority applies inside binary MWL files. A live Lunar Magic 3.63 re-export of a
Rust-installed sprite moved to upper-Y band 2 with an escaped `$FF` first byte retained sprite
header `$24`, `$FF 02`/`$FF FF` tokens, and expanded `$FF $FE` framing while the MWL container's
32-bit flags remained zero. The re-export also matched Rust's complete comparator-derived
screen/band order. Binary import must therefore select the
sprite codec from payload byte 0 bit `$20`; the top-level flags are opaque provenance and must not
be rewritten from sprite state. The legacy multi-file format is distinct: its `.mwl` manifest's
sprite-section flag selects framing for the separate `.mw2` payload but supplies no binary
container-flag value.

`SerializeLevelSpriteList` also canonicalizes in the opposite direction. A live oracle supplied
an expanded `$20` header and `$FF $FE` terminator around only ordinary records, with no upper-Y
transition, opaque control, or escaped `$FF` record. Lunar Magic 3.63 re-exported the identical
record sequence with `$20` clear and a single `$FF` terminator. Expanded framing is therefore a
serializer-derived capability requirement, not sticky level metadata: save paths must downgrade
when the last expanded-only token disappears and upgrade when one is introduced.

Direct object dragging now covers all 32 native screens. The canvas reverses
horizontal or vertical tile coordinates into an absolute screen plus the
selected ordinary record's first/second nibbles. `relocate_ordinary_object`
decodes absolute positions, removes only owned screen-jump controls, changes
the selected placement, stably sorts ordinary records by screen, and regenerates
the minimal transition: no bit on the current screen, the advance bit for the
next screen when representable, or a canonical first-low jump for other
transitions. Ordinary extension bytes and trailing opaque command-zero controls
remain byte-exact; interleaved unknown controls, invalid screens, control-record
selections, and off-canvas drops fail atomically.

`NativeLevelDocumentEditor` now has a stream-native placement canvas. The
legacy level mode selects horizontal or vertical axes; decoded object spans and
sprite positions determine a bounded, fixed-scale, two-axis-scrollable view.
For object-storage modes, the native frontend also decodes the recovered
`$02E600` Layer 2 pointer and paints that object stream first, so Layer 1
correctly overlays it and either layer can extend the canvas. `$FF`-bank
pristine sentinels remain absent rather than being interpreted as pointers.
`RenderLevelBackgroundMap16Canvas` at `0051C550` proves the compressed form is
a 32×32 Map16 plane with index `((y >> 4) * 31 + x) * 16 + y`: two
column-major 32×16 halves. The native canvas uses that exact bijection for the
decoded 1,024-word plane and paints it before both object layers. Those words
select Lunar Magic's distinct background definition namespace `$8000-$FFFF`,
not foreground definitions with matching low indexes. The installed preview
therefore loads the authenticated secondary Map16 blocks, tags tilemap
placements as background, and omits Acts-Like inspection for that namespace.
Definition lookup combines descriptor bits 4–6 (the active 4K bank) with each
stored word's low 12 bits. The renderer retains the complete raw word
separately, so lookup no longer consumes attribute or whole-cell flip bits.
Object-backed writes use the same typed placement boundary but preserve their
complete 15-bit foreground definition identity; their bit 14 is not a
background-cell X flip.
OSC custom-object display parts now preserve that same 15-bit identity when consulting the
fixed 1,024-definition M16 sidecar. Out-of-range `$4001` remains unresolved rather than being
masked into unrelated entry `$0001`, and its diagnostic marker retains all four hexadecimal digits.
The main level editor now also consumes compressed Layer 2 whole-cell flips instead of using them
only for definition masking. Shared-background precomposition reverses source pixels, ordinary
atlas cells reverse UV axes, and M16-backed cells permute quadrants and XOR outer/subtile flips.
Its standard-object cache and visual-catalog renderers also follow the OSC display renderer's
Map16 source boundary: only definitions below `$0200` address the shared vanilla atlas, the next 512
entries require an M16 definition, and unavailable or higher identities retain an unresolved
marker without low masking. Catalog selection and placed artwork consequently share the same source.
OSC catalog previews now retain that marker as well, including the complete four-digit identity,
when the M16 definition or its required texture cannot be drawn.
The native-assets Layer 2 panel now uses the same shared bijection to present a
clickable 32×32 Map16 grid. Selecting a canvas cell resolves its non-linear
storage index and loads the complete 16-bit word; applying it continues through
the existing revision-checked aggregate transaction rather than mutating the
frontend snapshot. Synthetic coverage proves all five boundary coordinates and
rejects both out-of-range coordinates and truncated tilemap storage.
Shift-selection now extends the anchor to an inclusive canvas-coordinate
rectangle. Rectangle fill enumerates cells in visual row-major order, resolves
each through the same recovered native bijection, and submits one duplicate-free
word-edit batch. Reversed drags across the `$01FF/$0200` plane boundary produce
the same indexes, while clearing the canvas selection restores exact raw-index
editing.
The GUI clipboard boundary uses a distinct `LMCLIP` kind for compressed Layer 2
rectangles. Its record stores one-byte width/height followed by little-endian
words in visual row-major order; native storage indexes never escape into the
portable payload. Paste anchors at the first selected canvas cell, converts all
words back through the recovered bijection as one aggregate edit, updates the
visible selection to the pasted extent, and rejects any rectangle crossing the
32×32 edge before controller mutation.
Cut/delete semantics are now static-evidence-backed. `HandleLevelBackgroundCharacterShortcut`
at `00523CC0` maps Ctrl+X to command `$2274`; `DispatchLevelBackgroundEditorCommand` at
`00524B30` copies the selection and then sends Delete `$2277`. Rectangle finalization at
`0051F040` zeroes all 1,024 transfer words, and `RestoreLevelBackgroundSelectedTiles` at
`005208B0` copies those words into every primary-selected live cell. The native GUI therefore
publishes the typed rectangle first and submits one atomic `$0000` fill batch; it does not guess
Map16 `$0025` or another visually blank tile.
Flood fill is now recovered from `FloodMarkMatchingLevelBackgroundRegion` at `00520B40`,
`FloodFillLevelBackgroundWithTilePattern` at `00520F10`, and
`ValidateAndFloodFillBackgroundMap16Pattern` at `00521110`. Lunar Magic compares complete
16-bit source words, marks a bounded four-connected region without wrapping at the 32×32
edges, and repeats the replacement rectangle from the region's minimum X/Y bounds after
masking every replacement word to its 12-bit Map16 index. The native panel implements both
the one-word specialization exposed by its word field and the complete rectangular operation.
It can retain any selected rectangle in visual row-major order, then repeat that pattern over
the destination region from the region's independent minimum X/Y bounds. Both paths resolve
the region in visual coordinates, mask replacements with `$0FFF`, and submit all affected
native-storage indexes as one aggregate transaction. An independent exhaustive oracle covers
every binary topology in a 3×3 neighborhood at every possible start cell, in addition to
full-word identity, disconnected islands, deterministic visual order, edge behavior, irregular
region bounds, rectangular repetition, and malformed pattern dimensions.
Rectangle relocation is now tied to the staged-selection pipeline at
`SwapLevelBackgroundTransferTilesAtOffset` (`0051F370`),
`TranslateLevelBackgroundPrimarySelection` (`0051F4E0`),
`MoveLevelBackgroundSelectionByPixels` (`0051F5A0`), and
`ClampLevelBackgroundSelectionPlacement` (`0051F7B0`). Lunar Magic snapshots selected words
outside the live plane, translates only in whole Map16 cells, and validates every selected cell
against the 32×32 boundary before committing. The native final-state model performs the equivalent
snapshot-first operation: clear the complete source rectangle to `$0000`, then place the captured
words at the destination so overlapping destination cells win. Directional GUI actions retain
reversed selection endpoints, update the non-linear storage cursor, and submit every changed source
and destination word as one duplicate-free aggregate transaction. Tests cover overlap, zero-delta
no-ops, all four crossed edges, malformed sources, and reversed selections.
Pattern resize follows `ClampLevelBackgroundResizeDelta` at `005202C0` and
`ResizeLevelBackgroundSelectionPattern` at `00520590`. Each drag edge is independently fixed or
moving; the clamp prevents inversion below one cell and prevents any new cell from leaving the
32×32 plane. Lunar Magic restores the previous selected cells, clears both transfer buffers, then
repeats the originally captured rectangle across the complete resized bounds from the new
top-left corner, masking each word with `$0FFF`. The native panel exposes one-cell grow/shrink
controls for all four edges and uses the same final-state contract: snapshot and clear the source,
tile the source pattern from the resized minimum corner, preserve endpoint orientation, and submit
one duplicate-free aggregate edit. Focused coverage proves left/top re-anchoring, overlap
normalization, removed-edge clearing, every crossed boundary, and the 1×1 minimum.
Each native axis grows from 16 through at most 512 tiles, so expanded sprite
upper-coordinate tokens remain visible instead of being clipped; strong
16-tile screen boundaries remain explicit. Ordinary objects render their
recovered footprint. Standard
sprites use the authenticated dispatcher with the real placement byte, level
mode, and orientation, displaying each composite part's recovered signed
geometry; unresolved/custom records remain clearly labeled markers. Hit
testing includes the complete standard composite and loads the exact object or
sprite semantic form. The explicit move tools convert horizontal/vertical
display coordinates back into the orientation-neutral native axes. Object
moves reuse the transition-preserving stream relocation transaction. Sprite
moves preserve the command identity, extra bits, extension payload, and record
priority order. For expanded streams the editor first resolves every record's
effective upper-Y state, changes the selected record, then emits the minimal
sequence of shared `$FF vv` transitions and returns the selected record's new
token index. Opaque `$FF 80..FD` controls remain a typed atomic rejection until
their state interaction is proved. Since `LMLVL1` carries no graphics,
palette, SSC, OSC, or Map16 sidecar payload, this canvas intentionally makes
no unsupported artwork claim.

The reciprocal MWL Wine oracle inserts an object, moves the original from its
source screen to screen `$02`, and changes both coordinate nibbles. Lunar Magic
3.63 imports and re-exports the exact canonically transitioned Layer 1 stream,
including level `$105`'s trailing four-byte opaque control.

The profile-backed `NativeLevelDocumentEditor` no longer limits installed or
custom sprite records to raw hexadecimal replacement. Loading an ordinary
record populates sprite number, screen, X, low Y, and extra-bit controls.
Applying them calls `set_native_fields` with the controller's exact externally
loaded 1,024-byte length table, so custom extension bytes remain untouched and
a field change that would select another record width is rejected before
document history changes. Upper-Y and other control tokens remain explicitly
raw and disable the semantic action.

The same interpretation-bound editor now loads ordinary object command,
parameter, coordinate, and resolved absolute-screen fields. Applying the form
stages command and parameter validation followed by `RelocateOrdinary` in one
object batch. Custom extension bytes survive both the shape-checked field edits
and any stable screen reorder; screen jumps and other nonvisible controls do
not masquerade as editable ordinary objects.

The native canvas also supplies the renderer's recovered two-bit animation
phase. It derives an 8 Hz phase from the GUI clock and requests 125 ms repaints
only while sprite `$A6` is present. This activates all four authenticated `$A6`
preview geometries without turning every static level canvas into a continuous
animation loop; invalid or pre-start clock values deterministically use phase
zero.

## Typed MWL Layer 1 interoperability

The MWL Layer 1 section uses the same two-word common prefix as sprites. Its
payload is the exact five-byte legacy level header followed by Lunar Magic's
terminated variable-width object stream. `MwlDocumentController` now exposes
that payload as `LevelObjectData` and replaces it as one revisioned canonical
transaction. Both opaque provenance words and all unrelated MWL sections are
retained; stale revisions, malformed records, and payloads exceeding the
single LoROM-bank limit fail without changing document history.

An ignored Wine oracle exports pristine level `$105`, duplicates its first
object through the typed Rust object-edit engine, imports the resulting MWL
with Lunar Magic 3.63, and re-exports the level. The complete decoded legacy
header and ordered object records match the Rust model exactly, and the
resulting ROM checksum remains valid. The native MWL window uses this same
controller boundary for exact header editing and ordered 3–8 byte
standard/extended/custom object insertion, replacement, deletion, and
movement. Selected records additionally expose the recovered distributed
six-bit command ID, command-specific parameter, orientation-neutral coordinate
nibbles, screen-advance bit, and both packed screen-jump encodings. These
field edits use the shared lossless `ObjectEdit` engine, preserve extension
bytes, and reject implicit record-shape changes; the GUI does not implement a
second toolkit-specific serializer.

## Standard-sprite dispatch coverage audit

The live `InitializeSpriteRenderDispatchTable` at `004cb250` proves that the
256-entry table is initialized to `004c3810` and then sparsely overwritten.
This matters because an absent Rust preview cannot automatically be classified
as Lunar Magic's native empty handler. A renewed table-to-renderer audit found
additional dedicated handlers inside the standard `$00`–`$ED` range.

The pristine 512-slot renderer audit exposed another false-empty entry: `$29`.
The live dispatch table routes it to `004c4d10`, where the packed placement's
low nibble must be `$C` and rows `$0`–`$6` select the Morton, Roy, Ludwig,
Iggy, Larry, Lemmy, and Wendy two-line boss labels. Other placements emit the
native `MAY GLITCH!` warning. The Rust renderer now implements all seven
branches; across the untouched ROM corpus all 3,290 sprite instances resolve,
with zero native-empty and zero unresolved instances.

The first corrected cluster covers `$80`, `$83`, `$87`, `$88`, and `$8B`.
Disassembly at `004c7680`, `004c7850`, `004c7af0`, `004c7b60`, and
`004c7c20` authenticates their complete definition indexes and signed pixel
offsets. `$83` selects `$801A`, `$8104`, `$8106`, or `$8100` from the low two
bits of the native first record byte; its three surrounding parts remain fixed.
`$87` and `$88` intentionally reuse the geometry of `$85` and `$86`, while
`$80` repeats `$7F` and `$8B` repeats the two-part `$DE/$DF` geometry. These
entries now render in both the shared Rust renderer and the native picker/canvas
instead of reaching the unresolved red marker. Tests enumerate all four `$83`
branches and every authenticated alias.

The contiguous `$64`–`$68` cluster is now recovered as well. Its handler entry
points are `004c69a0`, `004c6af0`, `004c6c80`, `004c6e10`, and `004c6f10`.
`$64` emits `$8E`, three or seven `$8F` middle segments, and `$9F`; the long
branch requires both first-byte bit zero and a `$64` selection in either of the
active context tables. `$65/$66` are opposite six-part arms, selecting the
`$8E/$9E` and `$201`–`$205` families from the same first-byte bit. `$67` selects
the `$AB`–`$BE` four-part family, and `$68` selects `$A9/$AA`. The latter four
handlers invoke `ValidateWideObjectHorizontalPosition` for their even branch;
the shared mode therefore carries explicit long-stem and invalid-wide-placement
inputs instead of silently assuming that hidden editor state.

The preview-definition table itself is embedded at executable file offset
`0x26F66C`, with four little-endian tile words per eight-byte entry. Index `$01`
reproduces the previously authenticated `$0400/$0410/$0401/$0411` definition,
which independently anchors the table base and stride. Direct table extraction
now supplies the formerly missing `$8E/$8F/$9E/$9F`, `$AD/$AE/$AF`,
`$BD/$BE/$BF`, `$11C/$12C`, and `$201`–`$205` definitions used by this cluster.
Tests cover the short and long `$64` stems, both first-byte branches of
`$65`–`$68`, alternate-number display, and validator rejection.

Remaining dedicated entries discovered by the same audit are tracked as
implementation work rather than being mislabeled as native empties: `$EF` and
`$F2`–`$FF`.

The remaining ordinary standard-range entries are now authenticated.
`$6C` at `004c7030` walks two cells left and adds the same final negative-eight
offset as `$6B`, producing its identical three-part `$B7` chain. `$9D` at
`004c8750` selects `$1BD`, `$1AD`, `$14`, or `$211` from the first record
byte's low two bits, then calls the same composite-tail helper used by `$9A`;
alternate-number display emits `$115`.

`$8A` at `004c7bc0` is list-stateful. `RenderAllLevelSprites` clears
`DAT_00617932` before walking the list. The first four standard `$8A` handlers
emit definitions `$110`–`$113` in order and increment that byte; later
instances append definition `$001`. The embedded definition table supplies
the exact four new definition records. Both Rust native canvases now maintain
this counter in placement order. An SSC custom-display override does not
consume it, matching the native path where the standard handler is never
called.

Custom level time is not part of the five-byte legacy header or the 32-byte
expanded-settings record. `DecodeLevelObjectStreamToNodeList` at `00435200`
intercepts object command `$28`, combines the two coordinate nibbles into the
low byte, and copies the third record byte into bits 8–15 of global
`DAT_008f3870`. Bit 15 is the dialog's force-reset flag and the low 12 bits are
the `$000`–`$FFF` timer. The decoder swaps the two low nibbles for vertical
orientation. `SerializeLevelObjectList` at `00435650` performs the reciprocal
mapping and appends the three-byte control immediately before `$FF`; it omits
the record when the complete value is zero. The Rust model and native editor
now expose this exact representation, including forced zero for infinite time,
and a live Lunar Magic 3.63 import/re-export preserves `$ABC` plus force reset
in both semantic models. `SaveLevelToRom` separately calls
`CheckLevelSaveSupportPatchB`/`InstallLevelSaveSupportPatchB` at
`00463950`/`00463990` when the global is nonzero. A matched, equal-length
control differential proves that installer replaces five `JSR $B3E3` operands
with `JSR $F160` and writes the fixed 48-byte `$0D:F160` runtime. The runtime
conditionally copies the Layer 1/2 scroll nibbles from `$57/$59` into
`$0F31..$0F33`; it returns without writes when `$59` bit 7 or `$141A` is set.
The apparent adjacent `$07EFA3..$07F08D` oracle difference is Lunar Magic's
additive checksum-compensation run (`$80`, then zero fill), not another timer
table. Rust now authenticates and installs the six fixed ranges atomically.
Exhaustive single-byte corruption tests cover every hook operand and runtime
byte in both recognized states; truncation and duplicate installation are
typed, and a deliberately failed final runtime precondition proves that none
of the five earlier staged hook writes or checksum/history effects escape.
The logical-offset transaction is also container-transparent: headerless and
512-byte copier-headered profile, native, and built-CLI gates retain the exact
physical header shape and bytes while authenticating the same installed ROM.

The installed ROM native-assets Level tab now exposes the complete five-byte
legacy header as typed controls: level mode, screen count, palettes, graphics
sets, music, preset/custom time, and Layer 1 vertical scroll. It emits the
whole form as one ordered controller batch. If `ChangeLevelModeDialogProc`'s
recovered storage classifier says the requested mode crosses the Layer 2
object/tilemap boundary, the ordinary batch remains failure-atomic and the UI
offers a revision-checked reset confirmation; approval alone replays the same
batch through the explicit reset route. A mixed-field commit/reopen regression
proves the header, custom-time control record, and reset Layer 2 representation
all survive installed-ROM persistence together.

Installed native-assets staging now keeps a bounded 100-step history of the
entire aggregate rather than domain-local fragments. Each accepted level,
Layer 2, palette, ExAnimation, feature, or expanded-settings batch records one
predecessor; failed and no-op batches record none, while divergent edits clear
redo. History snapshots include the active Layer 2 representation, dormant
object workspace, installed descriptor, and reserved-mode compatibility state,
so undoing and redoing a confirmed storage-class transition cannot silently
lose the object data needed by a later switch back. The native ROM workspace
exposes Undo/Redo before commit, disables them for stale or file-busy sessions,
and clears pending destructive confirmations whenever history moves. A
cross-domain regression commits the history-selected state and reopens the
same core and Layer 2 aggregate from ROM.

The shared portable/installed native-assets Level panel now exposes the same
lossless semantic record fields as the standalone native-level editor. An
ordinary object can edit its command, parameter, coordinate nibbles, and
resolved screen as one atomic object batch. A sprite record can edit its
number, screen, X, low five Y bits, and extra bits while preserving opaque
extension bytes. Both aggregate controllers expose the immutable sprite-length
table retained at decode, so semantic sprite replacement validates the exact
standard or custom record width instead of assuming three bytes. Control
tokens and objects without an ordinary placement keep semantic actions
disabled, and revision reloads invalidate previously loaded field forms. A ROM
commit/reopen regression proves combined semantic object relocation and sprite
field replacement survive installed persistence exactly.

Object-backed Layer 2 now uses the same semantic form contract in the installed
native-assets panel. Loading a positioned record resolves its native screen and
exposes command, parameter, coordinate nibbles, and screen; applying the form
emits one `Layer2Objects` batch containing the field edits and relocation.
Command-zero controls without a positioned placement remain ineligible, raw
editing remains available for opaque encodings, and revision/history movement
invalidates loaded semantic fields. The aggregate history regression now
undoes and redoes a semantic Layer 2 relocation before committing an additional
cross-domain edit, then reopens the exact object stream from ROM.

Aggregate semantic forms now follow selection without a separate load gesture.
Layer 1 objects, sprite tokens, and object-backed Layer 2 each retain the index
whose fields are currently loaded: changing that index reloads immediately,
while an unchanged selection preserves unapplied field edits across UI frames.
Every accepted edit, Undo/Redo move, import, or other controller invalidation
forces a canonical reload, clamps selections to the new stream endpoint, and
disables semantic fields when the selected value is absent or a control token.
Focused lifecycle tests cover selection changes, preservation of unapplied
values, control-token rejection, and canonical Layer 1/Layer 2 revision reloads.

The aggregate Level and object-backed Layer 2 panels now expose bounded Move
Up/Move Down priority actions for Layer 1 objects, Layer 2 objects, and native
sprite tokens. They translate the selected row through the same pre-move
`MoveBefore` indexing used by the pristine editor, so moving down skips the
element's old slot and moving up inserts before the preceding element. The UI
defers its selected-index change until the controller accepts the edit and
clears the pending move on rejection, preventing failed batches from drifting
selection. One aggregate regression reorders all three streams atomically,
undoes/redoes the complete state, commits it, and reopens the exact priority
order from ROM.

The Lunar Magic 3.63 `Change Properties in Sprite Header` dialog is now traced
from resource `$3F5` through its procedure at `$00412CE0`. Its two buoyancy
widgets are independent auto-checkboxes, not radio alternatives. On accept,
control `$1B4` clears/sets sprite-header bit `$80`, and control `$1B5`
clears/sets bit `$40`; initialization reads those same bits back into the same
controls. The Rust semantic names therefore intentionally do not follow
ascending bit order. The same handler also separates three adjacent settings
from the sprite stream byte: combo `$68` stores the horizontal-level vertical
spawn range in `DAT_005F1A46` bits 0–1, checkbox `$69` stores Smart Spawn in bit
2, and checkbox `$6A` stores the beyond-boundary air/water choice in bit 2 of
the packed high nibbles returned by `$00464B00`. `ExportBinaryMwlLevelFile`
places `DAT_005F1A46` at MWL level-header byte 6, while
`LoadLfix3LevelRuntimeFields`/`WriteLfix3LevelRuntimeFields` prove it is a
separate current-Lfix3 per-level plane in ROM. Those storage domains must remain
separate when the remaining controls are made semantic.

The bitmap conversion globals at `$005E55CC..$005E5600`, the palette-entry state map, and the four
Other Options numeric globals are process state rather than per-import scratch values. Reopening the
native dialogs restores prior accepted choices. Rust consequently retains the complete native
bitmap option value in the Map16 editor across preview cancellation, import, and editor reopen. The
asynchronous launch request captures that retained First Map16 value instead of deriving it from the
currently displayed page. Native previews also initialize the recovered eight-row color state immediately;
the former Rust-only single-row mode remains available to portable APIs but is not exposed as an
original Lunar Magic dialog choice.

The disposable Wine bitmap audit now sets controls `$74/$65/$66/$6B/$68/$6D` to independently
requested Boolean states before accepting Bitmap Pasting Other Options. The retained inverted-state
capture observed `DAT_005E55F4..DAT_005E55F8 = 00 00 00 00 00` and
`DAT_00E27B31 = 01`, directly confirming that each checkbox owns the documented byte and that the
dialog's OK path persists all six choices before conversion.
Edit controls `$67/$69/$6C/$6E` own first 8×8, blank 8×8, first Map16, and reserved blank Map16.
The live `$220/$0F9/$8300/$8001` submission produced globals `$00000220/$000000F9/$00008300/
$00008001`; the changed graphics digest also proves the first-tile value reached conversion rather
than merely repainting the dialog. The audit validates the native 10-bit graphics and 16-bit Map16
ranges before launching Wine and records requested and observed values separately.

`ImportMap16PageFromSnesFiles` at `$004E5390` implements the distinct "SNES GFX Set + SNES
Screen Tile Map" workflow. For pages below `$100`, it zeroes an `$8000`-byte graphics buffer,
reads at most that amount, and decodes 1,024 4bpp tiles. It then reads an `$0800`-byte 32×32
little-endian screen map. Each word's low ten-bit source tile is translated through
`g_awGraphicsTileRemap`; the referenced decoded graphics tile is copied to that destination and
the word's `$FC00` attribute bits are retained. Traversal order is significant when remap entries
alias: the last screen-map reference wins. Map16 definition `n` uses screen-map offsets TL
`(n >> 4) * $40 + (n & $0F) * 2`, TR `+1`, BL `+$20`, and BR `+$21`. The direct path replaces
only these four words, preserving destination Acts Like values; the alternate path deduplicates
definitions into blank entries. If enabled, the workflow asks for a `.col`/`.pal` file and
`LoadPaletteRowFromFile` at `$004765E0` reads exactly `$20` bytes into the selected working palette
row. Background-page optimized import additionally calls `PasteMap16IndexGridIntoLevelLayer` at
`$00519010` with a 16×16 grid at destination `(0,0)`. The helper accepts dimensions 1..32, rejects
an index grid without an assignment in active bank `(background_page_bank + 8) * $1000`, masks
stored words to twelve bits, and addresses the level buffer as
`((x >> 4) * 31 + y) * 16 + x`. The direct-definition path does not invoke this paste.

The bundled 8×8 editor documentation closes the imported graphics ownership ambiguity. Its first
three pages are six `$80`-tile FG/BG slots in VRAM order `FG1, FG2, BG1, FG3, BG2, BG3`; the fourth
page (`$300..$3FF`) is normally blank and unavailable, while SP1–SP4 occupy the following two pages
outside this importer workspace. Super GFX Bypass exposes the same six files in dialog order
`FG1, FG2, FG3, BG1, BG2, BG3`, so materialization swaps the FG3/BG1 positions when constructing
the native workspace and never attributes the unavailable page to sprite files.

The previously unpromoted options callback at `$004E4E00` clarifies the remap ABI. On first open it
calls `InitializeIdentityTileRemap` at `$004E4DD0`, which fills all 1,024 words at `$00963F78` with
their own indexes. Accept stores the two hexadecimal offset controls in `$005E5678` and `$00E27A98`.
The importer adds those values to each source tile, masks the sum to ten bits, and indexes the remap
table; identity is therefore the exact initial native state rather than a Rust approximation. The
optional color-map selector is one-based in `$00E27A9C`. After all graphics copies, the importer
visits each referenced remapped destination only once and calls `ApplyColorMapFilterToGraphicsTile`
at `$00503CE0` with selector minus one. That function maps each of the tile's 64 low-nibble pixel
indexes through the chosen 16-byte row of the 16×16 table at `$0091C580`, then re-encodes 4bpp.

The live Wine interaction is retained under
`oracle-work/lm363/pristine-us/snes-tileset-import-direct`. The hidden Map16 shortcut is dispatched
from the render child around `$00500356`; its Ctrl/Shift/Alt/F1 guards exit through the conditional
branches at `$00500375/$00500382/$0050038F/$0050039C`. The audit temporarily neutralizes only those
four branches in a disposable process, posts Insert to the authenticated child handle at
`$009B958C`, and immediately restores the exact instruction bytes. The native options dialog uses
control `$70` for Optimize and initializes it checked. Direct import then copies `$200` dwords to
`$00777E58 + page * $800`; selected page comes from `$00E27AE0 >> 4`. The retained page-zero run
records 5,491 graphics-byte and 1,819 Map16-byte differences and preserves both complete buffers,
dialog snapshots, and their hashes so the observation can be independently rechecked.

`LoadCustomOverworldSpriteSidecar` at `$005438A0` constructs the ROM-adjacent `.sscov` name and
accepts an optional UTF-8 BOM. Sprite IDs `$00..$FF` combine with type bit `$10` to address the
custom `$100..$1FF` namespace. Type bit 1 selects a point-based display definition; bit 0 supplies
the overworld shadow on those definitions and disables original position-dependent tooltip text on
description records. Display points are decimal signed X/Y plus hexadecimal Sprite Map16 tile,
where bit 15 requests translucency and the base tile may not exceed `$CFF`; at most 256 points and
absolute offsets through `$2FFF` are retained. Positioned `*text*` labels use the same escape
handling. Sentinel IDs `$10000/$20000` install tile-range mappings through `$BFF` for external
graphics and palettes. Repeated sprite definitions free and replace the earlier allocation, which
the Rust ordered-map codec reproduces semantically. The official help independently names `.sscov`
and `.s16ov`; the prior `.ovssc` ledger spelling was incorrect.

`LoadOverworld16x16SpriteSidecar` (`00544250`, reached from the `.s16ov` string xref at
`005d59f8`) clears exactly `0x4000` bytes before reading at most that many bytes from the
ROM-adjacent sidecar. This distinguishes it from the larger ordinary `.s16` store. Together with
the native Sprite Map16 page layout, those bytes supply the eight custom pages `$400..$BFF`; the
four preceding pages are built in, while `$C00..$CFF` remains Lunar Magic-internal display space.

The built-in source is authenticated by `LoadBuiltInOverworldGraphicsResources` (`004BF9A0`): it
calls `FindResourceA` with type `500` and ID `508`, then copies exactly `0x2000` bytes into the
four-word definition table at `00AFF968`. PE resource inspection reports the same 8,192-byte
payload at RVA `00AB0E6C`; its retained SHA-256 is
`d23b64559ac8a95d2011842cd4731f29914a45ac94cc74e7beff80ed54037d4b`.
`RenderOverworldLinkedTileOverlays` (`00541180`) indexes that table with each render-grid node's
15-bit tile word. Rust therefore resolves `$000..$3FF` from the exact retained resource and
`$400..$BFF` from `.s16ov`, rather than substituting ordinary level Map16 definitions.

The same live decompile now proves both external-resource routing tables. During
`InitializeOverworldEditorModel` (`005446D0`), native Sprite Map16 `$000..$BFF` receives graphics
base `$1C00` and palette sentinel `$FFFF`; internal `$C00..$CFF` receives `$3100/$FFFE`.
`LoadCustomOverworldSpriteSidecar` transforms graphics range bases by `kind & 3`: kinds 0, 1, 2,
and 3 add `$4200`, `$0000`, `$1C00`, and `$2A00`, respectively. An adjusted base at or above
`$4600` is ignored. Palette bases at or above `$0400` are likewise ignored. Accepted inclusive
ranges write one constant route per native Sprite Map16 index, with later records overwriting
earlier records. `RenderOverworldLinkedTileOverlays` reads both tables before resolving each 8x8
subtile, so routing belongs to the parent Sprite Map16 reference rather than its subtile number.

`HandleLevelEditorCommand` cases `$5D/$5E`, reached by commands `$2410/$2411`, toggle
`00E278ED/00E278EE` and invalidate the Map16 viewport. `RenderMap16TileToPixelBuffer` resolves any
Map16 value above `$1FF` through its Acts Like root, selects the 512-byte surface table at
`007586E8` or line-guide table at `00770C08`, and alpha-keys a 16×16 glyph from bitmap resource
500/524. That resource is an exact 1,808×16, 24-bit strip: 113 glyphs, with magenta transparent.
`InitializeVanillaAnimatedTileOwnershipMap` at `004653B0` is therefore more precisely the surface
outline lookup initializer; it also installs object-tileset-specific substitutions.
`InitializeAnimationTriggerIndexMap` at `00465350` is the line-guide lookup initializer: roots
`$76..$93` map to glyphs `$51..$6E`, `$96..$99` map to `$6F`, and the pristine conditional maps
root `$95` to `$62`.

The remaining level zoom commands are now identified in the internal-name table and central
dispatcher. `LM_VIEW_ZOOM` maps to `$2440` (case 99) and anchors popup menu `00816B14` at the
invoking toolbar button. `CreateEditorZoomPopupMenu` at `0048B471` appends nine radio commands
`$244A..$2452` from `005E7B34`: 100, 125, 150, 175, 200, 300, 400, 600, and 800 percent. It then
adds `$2448/$2449` zoom adjustment, `$2444` Zoom Filter, and the separate `$2445` automatic state.
`LM_VIEW_ZOOM_FILTER` maps to `$2444` (case 100), toggles `005E7B0C`, synchronizes its menu check,
and redraws the level surface. The executable initializes that byte enabled. The renderer's
presentation path distinguishes filtered scaling only after the editor surface is composed. This
rules out per-atlas linear filtering in Rust: Map16, sprite, and outline atlases place unrelated
cells adjacent to one another and would bleed at their UV boundaries. The native command/check
state is implemented; exact filtered final-surface presentation remains a compositor task.

The published animation names resolve through the same table to `$2404`, `$2403`, and `$240E`.
Case `$52` toggles playback byte `005E7B0A`, updates the ExAnimation timer, and pauses or resumes
LMSW. Case `$51` directly invokes `HandleExAnimationTimerTick` at `0045ACF0`, which advances
ExAnimation once and redraws dependent surfaces. Case `$5B` calls
`ReloadCurrentLevelGraphicsAndRedraw` at `00465320`; that function refreshes the palette, decodes
the loaded graphics caches, and posts a redraw, but does not zero animation counters. Rust
therefore shares one pausable clock across the canvas and sprite catalogs, advances one 60 ms tick
for `$2403`, and preserves the frame while rebuilding graphics for `$240E`.

The four published switch-state names map to `$23FB` through `$23FE` and toggle bytes
`005E7B02` through `005E7B05`; pristine executable data initializes all four bytes to one.
`PlaceConditionalSingleTilePatternA` at `004251C0` consumes the green byte for extended selector
`$87`, choosing `$06A` while clear or `$16A` while set. `PlaceConditionalSingleTilePatternB` at
`004254B0` consumes yellow for selector `$8E`, choosing `$06B/$16B`. Standard-object definition
slots 24 and 25 at `0042AA50` and `0042B440` consume blue and red respectively, filling the encoded
rectangle with `$06C/$16C` or `$06D/$16D`. Rust exposes the four flags as one typed default-on
render state and reapplies it to definitions shared by canvas and catalog rendering.

`LM_VIEW_SILVER_POW` maps to `$2405`, whose dispatcher case `$53` toggles default-zero byte
`00E278DE`, rebuilds the initial ExAnimation frame, and rebuilds sprite previews. That byte is read
throughout the standard-sprite dispatch cluster; for example `RenderConditionalTiles13And23` at
`004C3ED0` emits its ordinary `$13/$23` pair while clear and definition `$115` while set. Rust's
already authenticated standard handlers expose the same branch as `alternate_display`; the native
editor now supplies the toolbar state consistently to the canvas, existing-sprite picker, and new
sprite catalog. `AdvanceVanillaAnimatedTileGroup` at `00459C80` also consumes this flag when a
mode-one group's trigger byte is one, so Rust applies it to vanilla animation group 9 as well.

`LM_VIEW_POW` maps to `$2406`; dispatcher case `$54` toggles default-zero byte `00E278DD`, updates
the menu check, rebuilds the initial animation state, and redraws the dependent editor surfaces.
`LoadExAnimationFormatState` at `004596F0` loads the pristine mode table and its overlapping trigger
view. In logical ROM coordinates the 24 mode bytes begin at `$02B96B`; the trigger table base is
`$02B97D`, making the eight consumed entries for mode-one groups 6–13
`0,0,0,1,0,2,2,0`. `AdvanceVanillaAnimatedTileGroup` and
`RenderVanillaAnimationGroupFrame` at `0049DA10` agree on the selector: trigger zero uses Blue POW,
trigger one uses Silver POW, and trigger two uses the inverted On/Off state. Groups 6, 7, and 10
also select replacement bank `$26` while default-on Invisible POW Objects byte `005E7B06` is set;
the executable image initializes that byte and On/Off byte `005E7B08` to one. Rust retains those
ordinary defaults, exposes the two independent POW flags, and authenticates both the normalized
ROM bytes and every selected source index in tests. Raw copier-header containers place these same
tables 512 bytes later; those container offsets are deliberately not used as logical PCs.

The adjacent user-toolbar name table establishes `LM_VIEW_INVISIBLE`, `LM_VIEW_INVISIBLE_2`,
`LM_VIEW_LINE_ON`, and `LM_VIEW_CDM16` as commands `$2400`, `$2401`, `$2402`, and `$2409`.
`CreateMainApplicationMenu` at `00447540` shows their initialized default-on states through bytes
`005E7B06`–`005E7B09`; `$2402` temporarily inverts its byte while
`InitializeAnimationTriggerIndexMap` at `00465350` rebuilds the persistent visible lookup, then
restores the control byte. `RenderMap16TileToPixelBuffer` at `0044EAF0` proves the display rules:
other-invisible mode maps `$021/$022->$114`, `$023->$113`, and `$024->$115`; Invisible POW mode
half-blends `$027-$02A` only while Blue POW is clear; and custom DSC flag bits two/four select the
corresponding display mapping. `$06F-$072` instead select four 16×16 half-blended bitmap cells.
`LoadEmbeddedLevelEditorBitmapPayloads` at `00498D00` identifies their source as PE resource type
500, ID 501. Parsing the executable resource directory locates its exact 64×16 24bpp strip; its
192 blue pixels are the transparent key and the remaining 832 pixels use the recovered six-color
histogram now authenticated by Rust tests. Finally, `PlaceRectangularLevelObjectTiles` at
`00421E10` proves that a `$27/$29` Direct Map16 record selects source tile `+$100` while `$2409`
is active only when both its output-width high bit and source-control high bit are set. Rust keeps
these as presentation state, applies them to the shared atlas,
animation selector, and per-object painter path, and never rewrites the authored record merely to
change the view.

`LM_VIEW_512HEIGHT_BG` maps to `$2407`; dispatcher case `$55` toggles persisted byte `005E7B0D`
and redraws without rebuilding level data. `RenderLevelEditorViewportRegion` at `004530A0` uses
that byte to change the background row modulus from `$1B0` to `$200` pixels.
`RenderTransparentLevelBackgroundMap16Tile` at `0051D1B0` independently changes its source-row
divisor from `$1B` to `$20` Map16 rows. Rust therefore applies one default-off presentation flag
to the composed game-preview plane and the direct Map16 fallback, preserving the 512-pixel
horizontal period and all authored tilemap bytes.

`LM_VIEW_TRANSLUCENT` maps to `$2415`; dispatcher case `$62` toggles byte `005E7AD9`, updates its
check state, and posts a redraw without rebuilding level data. The flag is shared by selection,
grid, screen-label, exit, warning, entrance, and Map16-page annotation paths.
`DrawClippedEditorTextWithBackgroundBlend` at `00451540` demonstrates the exact operation: while
set it saves the covered background, draws the ordinary overlay, then replaces every resulting
pixel with the packed-channel half average of the saved and drawn surfaces. Rust scopes a single
half-opacity painter to the equivalent editor-only overlay calls; artwork and interaction geometry
remain unchanged.

`LM_VIEW_BLOCK_CONTENTS` maps to `$2413` and toggles persisted, default-zero byte `00E278E0`.
`BuildMap16CustomDisplayMappings` at `00465930` resolves each cell's Acts Like root, honors `.dsc`
alternate mappings, and otherwise selects the recovered built-in definitions and position tables;
its output retains `$4000/$8000` composition selectors. `RenderLevelEditorViewportRegion` at
`004530A0` first draws the ordinary Map16 cell, then passes a synthetic one-node display rooted at
`00836B60` to `RenderM16SidecarObjectsToPixelBuffer` at `0044F670`, proving that contents are a
transparent editor overlay rather than a replacement Map16 tile. Finally,
`LoadEmbeddedLevelEditorLookupResources` at `00498D90` locks PE resource type 500, ID 502, and
`ValidateAndInitializeOpenedRom` copies its exact 0x2000-byte default `.m16` bank into the active
renderer before an optional ROM-adjacent sidecar overrides it. Rust retains that authenticated
bank byte-for-byte, including editor-only definitions `$219/$21A`, and builds an animated,
transparent 32×32 overlay atlas from the current level graphics and palette.

`LM_VIEW_BLOCK_EXITS` maps to `$2412` and is the adjacent default-off view backed by byte
`00E278E1`.
`BuildMap16CustomDisplayMappings` sets per-cell flag `$20` for Acts Like roots
`$01F/$020/$027/$028/$137/$138/$13F`, plus `$09C` only in level mode 1; `.dsc` flag-eight
records can add the same marker for either the source or resolved root. After all layer artwork,
`DrawInvalidMap16CellWarnings` at `004527C0` scans the final 0x3800-cell buffer, temporarily forces
translucent-overlay state off, and calls `DrawEditorSelectionOutline` at `00450B30` with black
outer and red inner colors. On a 16×16 cell that routine writes black edge lines at offsets
0/3/12/15 and red lines at 1/2/13/14. Rust mirrors the built-in root/mode predicate, resolves
final object-cache cells rather than historical writes, and paints the same opaque eight-line
outline only in editor view.

`LM_VIEW_HAVE_STAR`, `LM_VIEW_TIME_100`, and `LM_VIEW_5YOSHI_COINS` map respectively to `$240B`,
`$240C`, and `$240D`. Dispatcher cases `$58..$5A` toggle default-zero bytes
`00E27897..00E27899` and request redraw mode 4, which rebuilds the initial ExAnimation frame and
the entire level-editor surface. `ProcessExAnimationRecordGroup` at `00459F60` consumes these
bytes for trigger types 4, 5/6, and 7/8. Rust retains the three independent default-off preview
states, invalidates the graphics preview when they change, and exposes the original user-toolbar
tokens without mutating authored ExAnimation records.

The trigger-preview toolbar command families occupy `$2648..$26A1`. Dispatcher cases `$C2/$C3`
toggle 16 custom-trigger and 32 one-shot-trigger bytes; `$C4/$C5` increment or decrement one of 16
manual-trigger frame bytes with byte wrapping. Cases `$C6..$CB` wrap the selected custom, one-shot,
or manual index over 16, 32, or 16 entries, and `$CC..$CF` forward the corresponding current-index
action. Every state-changing action uses redraw mode 4. Rust parses the complete hexadecimal token
families, retains the same independent arrays and selectors, applies the same wrapping rules, and
invalidates the graphics preview only for actions that change rendered trigger state.

`ApplyExAnimationDialogToSelectedSlot` at `0040A9C0` writes the Trigger combo selection to record
byte 2. `ProcessExAnimationRecordGroup` switches on that same byte to select ordinary, POW,
On/Off, Star, timer, Yoshi coin, manual, custom, or one-shot behavior. The table at `005E77D8`
does not redefine the byte as a size mode; it reports whether a trigger requires two frame banks.
Rust now exposes byte 2 as `trigger()` and labels it Trigger in every ExAnimation editor, while
retaining `size_mode()` only as a source-compatible alias for older callers.

`ResetExAnimationRuntimeState` at `0045E900` calls `ReconcileExAnimationTriggerState`, which writes
`$FF` to every record's hidden cursor byte at offset `$208`. `AdvanceExAnimationFrames` at
`0045AAC0` processes one of eight interleaved record groups per substep. The recovered record loop
advances ordinary types cyclically, terminates types `$18..$1B` at `$FF`, selects a second frame
bank for conditional triggers, forces manual-trigger frames, and consumes one-shot trigger bits
after their last frame. Rust now has a stateful, resettable `ExAnimationPreviewState` that mirrors
those cursor, phase, bank-selection, manual, custom, and one-shot rules without storing runtime
cursor bytes in authored records.

The trigger-width table at `005E77D8` marks triggers 1–5, 7, 9–E, and `$20..$2F`. For those
records, `ProcessExAnimationRecordGroup` selects the triggered source by adding
`frame_count_minus_one + 1` to the ordinary frame index before reading the word at record offset
8. This proves the payload is two contiguous, bank-major source arrays, not adjacent word pairs.
Rust's shared frame decoder and editor now present each logical frame as `[normal, triggered]` but
decode and re-encode the authenticated bank-major byte layout, including insert, remove, reorder,
and replacement operations.

The graphics-transfer size table at `005E78D8` gives byte counts `$20,$40,...,$100,$180,...,$400`
for kinds 1–E and `$10,$20,$40,$80` for kinds F–12. `ProcessExAnimationRecordGroup` divides those
counts by 32 for complete 4bpp copies or by 16 for 2bpp extraction. In the latter path each source
4bpp tile yields a low-two-bit tile followed by a high-two-bit tile; kinds `$10..$12` repeat the
operation into a second destination block at tile `+16`. Rust now materializes these exact tile
overrides with complete source and destination preflight, preserving atomic failure behavior.

For palette kinds `$13..$15`, the same record loop treats a one-color transfer as a literal BGR555
word and a wider transfer as a contiguous palette copy. Kinds `$16/$17` write the fixed background
color instead of a CGRAM entry. Kinds `$18..$1B` rotate the destination range: `$18` right and
`$1A` left, while `$19/$1B` swap direction under the alternate trigger state. Rust represents
palette and fixed-color results as distinct transfer variants, validates every source, destination,
and literal before publication, and emits exact ordered palette overrides for all four rotations.

For absolute graphics sources, the record loop maps words `$2000..$7CFF` to cache tile `$900+`
and `$7D00+` to tile `$600+`. Relative-source records add `source_word / 32` to the active slot
base after checking the complete transfer against its word limit. Destinations below `$4000` use
16-byte tile addressing (optionally doubled for 2bpp support), `$4000..$5FFF` map at 8-byte
granularity to tile `$1C00+`, and `$6000+` map back to tile `$400+`. Rust exposes the same bounded
address resolver. As at `0045A2A2`, an absolute source below `$2000` or a relative transfer beyond
its setting's limit falls back to working-cache tile zero; invalid destinations remain rejected.

The profile-backed installed-level preview now loads its staged `CompactExAnimation` from the same
`NativeLevelAssetsController` used by edit, commit, and reopen. It builds Lunar Magic's working
cache with ordinary FG/BG tiles at `$000`, sprites at `$400`, GFX33 at `$600`, GFX32 at `$900`,
and the setting-selected relative bank at `$C00/$1000/$1400/$1800`. Records execute in the
recovered eight-way phase order against one mutable cache, so earlier destinations can feed later
sources. Tile, palette, and fixed-color transfers are applied to the rendered assets, and a staged
nonempty level set keeps the preview clock active when the Level ExAnimation feature is enabled.

`AdvanceExAnimationFrames` at `0045AAC0` establishes the composite scheduling contract. Each
substep calls `ProcessExAnimationRecordGroup` for the 32-record global array before the level array.
When global animation is active, the level count is 32; otherwise Lunar Magic passes 64. Rust's
composite preview state preserves that domain order and those limits while keeping independent
cursor and trigger state for the two arrays. The installed level preview currently supplies its
staged level set through this shared state. The installed global provider follows the selected
runtime hook already authenticated by the revision profile, changes its chained-operand displacement
to the split bank byte at `$5C` and low word at `$65`, and treats their masked-zero combined
24-bit value as Lunar Magic's intentional empty global set.
Present tagged or bounded payloads decode through the same compact-record limits as per-level data,
then execute before the staged level records against the same mutable tile and palette cache. The
same resolved `$5C`/`$65` operand pair is now the publication target for global edits: canonical compact bytes
are allocated copy-on-write, the runtime pointer and checksum publish in one transaction, optional
reclamation requires exact prior RATS ownership, and absent or unauthenticated runtimes remain hard
errors rather than implicit installations.
The native ROM ExAnimation editor consumes this boundary directly. It can switch from the selected
level slot to the global domain, uses the same typed setting, trigger, record, frame, and clipboard
edits, treats a zero runtime pointer as a new empty 32-record document, and commits through the
runtime-relative pointer with revision checks. Target switching is disabled while the active
domain is staged, preventing an application commit from silently discarding edits in the other
domain.
Because the bank byte at `+$5C` and low word at `+$65` are allocator-dependent mutable ROM state,
the profile's ROM-aware allocation policy resolves and protects both split operands alongside the
existing level-table locator, feature-table locator, and allocated tables. This is required before
the global editor can safely accept a search range that happens to span the installed runtime.

`EnsureExpandedExAnimationRuntimeInstalled` (`0045FD50`) also proves that installed editing is not
the entire lifecycle surface. It distinguishes a legacy pointer-hook form, a legacy 512-entry
animation-table form, and no runtime. The first routes through `PatchLegacyExAnimationPointerHooks`
(`0045E5F0`); the second routes through `MigrateLegacyGlobalExAnimations` (`0045F980`), which
installs the current runtime, converts up to 512 legacy `$23`-byte-record blocks, reallocates compact
payloads, and erases authenticated obsolete storage; the third performs a fresh
`InstallExpandedExAnimationRuntime`. These installation/migration workflows remain a separate
implementation gate from editing an already installed current runtime.
An authentic Lunar Magic 1.65 legacy-global ROM sharpens the second branch. Its runtime entry at
logical `$086E10` begins inside the `$140` auxiliary allocation rather than at the owner's payload
start, and the runtime prefix intentionally overlaps that table before the migration erases it.
The separate `$600` table contains four populated slots. A reciprocal Lunar Magic 3.63 import
places the current core at irregular destination `$0A5D51`; all twelve ordinary IRAM operands then
equal the mapped runtime low word plus `$05AF/$05AF/$0B4A/$05CF/$0B82...`, proving they are
allocation-relative fixups rather than fixed values from the usual `$080549` placement. Rust now
models that ownership, overlap, complete fixed-hook replacement, and all twelve relocations; the
all-512-slot semantic comparison and hashes are retained in
`docs/oracle-work/expanded-exanimation-legacy-global-165.md`.
The optional branch predicate is
`(DAT_00E278FC && DAT_00E27901) || (DAT_00E278FD && DAT_00E27903)`. A direct Ghidra memory read
authenticates its `$20` bytes as `A5 03 8D 20 C0 A5 05 8D 22 C0 A5 08 5C 20 C0 7F 4C 4D 00 01`
followed by twelve `$FF` bytes. Rust now retains that exact suffix as a distinct checked asset and
can materialize the original contiguous `$C30` or `$C50` stack-buffer forms. The open-ROM
initializer closes the predicate's persisted meaning: ExLoROM declaration bit 1 selects feature
bit 17, while SA-1 declaration bit 2 selects feature bit 18. If the relevant declaration bit is
absent, `CheckLegacyRomMappingHook` reads the active SMW-US descriptor's two-byte word at logical
`$002D2B` and returns true exactly above `$1FFF`. The ROM-aware Rust selector implements both
metadata paths, the exact legacy threshold, partial-metadata rejection, and the invariant that
ordinary LoROM never selects the suffix. Transactional mapper placement remains the installation
gate.
Disassembly at `$0045E519` closes the suffix-specific relocation boundary. When the suffix length
is nonzero, Lunar Magic writes the mapped allocation address of core `+$C30` into the three-byte
pointer slot at core `+$78A`, then writes the fixed mapper-compatibility target `$7FC020` into the
pointer slot at core `+$792`. Rust models both as checked 24-bit values, keeps the ordinary `$C30`
form unchanged, and rejects either value above the SNES address width. The preceding conditional
pass is now bounded too: after the 108 internal-address fixups, it validates 37 embedded IRAM words
below `$2000` and adds `$6000`, then validates three compact words below `$0100` and adds `$3000`.
The first group spans core offsets `$15B..$A5E`; the compact group is `$47C/$78A/$792`, with the
last two subsequently replaced by the mapped 24-bit suffix/helper pointers. Rust preflights all 40
values before modifying any byte, reproduces that exact ordering, and rejects a late invalid value
without a partial transformed payload. The relocatable payload builder now carries that complete
`$C50` form through concrete ExLoROM and SA-1 allocations: all eight external pointers use the
canonical mapper address rather than LoROM's low-bank mirror, all 108 local words resolve against
the allocated core, the suffix self-pointer targets the concrete `+$C30`, and direct reconstruction
matches every installed payload byte. The complete mapper plan now also installs both allocated
graphics helpers, the missing-graphics sentinel, both canonical shared-palette hooks and their
`+$6000` IRAM-adjusted payloads, and the previously omitted `$025E1` NOP pair. Independent current-
runtime detection resolves every hook with the selected mapper, authenticates exact RATS ownership
and `$C50` shape, reconstructs the complete relocated core and suffix, and rejects corruption in
the core, suffix, allocated helper, or fixed writes. ExLoROM uses the conversion's active relocated
SMW body at logical `+$400000` for metadata, generation signals, hooks, sentinel, and fixed helper
payloads; allocations remain in its canonical expanded lower half. The native application selects
the exact metadata/legacy predicate and complete mapper plan. Eight ExLoROM/SA-1 permutations cover
the `$C30` and `$C50` forms with and without copier headers, strict current detection, checksum-
valid save/reopen, duplicate rejection, header preservation, and byte-exact Undo. The authentic
mapper Oracle is now retained too: Lunar Magic 3.63 command `$23B4` converted the authenticated
pristine SMW-US ROM to an 8 MiB copier-headered ExLoROM image, then `-ImportLevel` imported the
active-ExAnimation level fixture. The complete images have SHA-256 values
`3c8e26cce4ea5d7741e499d4533565229b152ac42450b057ab204fb9f5ebb890` before and
`76ffb9a832d9b9f984c083f3cdbec025fbb2e1f1f06d98b36ae8ef77df008126` after. A compact
4,810-byte fixture (decoded SHA-256
`7c5cbde5431267017daa0fe45c7065d1edae9397f6afd833d89307aaf5190edb`) retains every owned
runtime-family byte without redistributing either ROM: the `$C30` core at `$200549`, live `$600`
pointer table at `$201181`, `$20`/`$30` allocated helpers, core and pointer hooks, NOPs, sentinel,
both shared-palette hooks and payloads, and both allocated-helper hooks.
`authentic_lunar_magic_exlorom_runtime_family_matches_every_owned_byte` reconstructs each
allocation-dependent address and requires exact equality across that entire family. Authentic
legacy-generation migration before/after fixtures remain the Oracle gap.
The fresh installer's core allocation is now recovered byte-for-byte. It concatenates executable
ranges `$005B5298..$005B5408` (`$170` bytes), `$005B5410..$005B5750` (`$340` bytes), and
`$005B4B10..$005B5290` (`$780` bytes) into one `$C30`-byte payload; a mapper-specific branch may
append the distinct `$20`-byte range at `$005B5754`. The typed Rust relocation model covers the two
mapping bytes, eight 24-bit SNES pointers, twelve 16-bit internal-RAM operands, and all 108 local
address words beginning at payload `+$B4A`. Relocating the complete template to the retained Lunar
Magic 3.63 allocation at logical ROM `$080549` reproduces every `$C30` payload byte exactly. The
separate empty `$600`-byte pointer table is also modeled as 512 repetitions of `FF 00 00`; this is
published with the runtime through one typed relocation plan. The plan uses eight low-bank 24-bit
fixups, 108 low-word local fixups, and the authenticated pristine `$0283AD` hook. Ghidra then proves
that the same installer calls `AllocateAndInstallLevelGraphicsRuntime` for the `$20`-byte
`$005B4AB8` template, initializes the 16-byte missing-graphics sentinel, writes both fixed shared-
palette helpers and hooks, and calls `AllocateAndInstallGraphicsRuntimeBlock` for the `$30`-byte
`$005B4ADC` template. Those dependent writes now belong to the same Rust plan. It reproduces all
four retained allocation ranges through `$0817E1` and every affected fixed range byte-for-byte,
repairs the checksum, preserves copier-header framing, rejects a changed hook before expansion or
history, and undoes exactly. Earlier general-save prerequisites and the later imported-level
payload remain separate transactions; the optional mapper-specific `$20` suffix remains a variant
gate.
Disassembly also bounds the pointer-hook-only generation exactly. Descriptor entry `$16A`
identifies the long-call operand whose resolved runtime owns the legacy payload. The migration
accepts marker `4C 4D 00 01` at runtime `+$169`, writes bank `$10` at `+$92` and `+$118`, and
advances only the marker's generation byte to produce `4C 4D 01 01`. Rust authenticates the JSL
target, containing RATS owner, payload extent, and marker before constructing three guarded writes.
Headered and headerless tests prove checksum-valid publication, no allocation or expansion,
late-change atomicity, and byte-exact undo.
The generation probe does not reduce current-runtime detection to the `$0283AD` opcode. It resolves
and authenticates the core and `$600` pointer-table RATS owners, reconstructs every immutable core
byte from the concrete allocation addresses, retains only the feature/global-pointer operands that
the editor legitimately mutates, and authenticates both allocated graphics helpers plus the fixed
sentinel and shared-palette family. Generated and retained Lunar Magic current runtimes pass; core,
allocated-helper, and fixed-helper corruption each reject instead of falling back to absence.
The active SMW-US descriptor also resolves the previously omitted `$1E1` hook to logical `$02390`.
Fresh installation writes `JSL core+$170; RTS` there; the retained Lunar Magic output and Rust plan
now match all five bytes, and current-runtime detection authenticates its allocator-dependent target.
`EnsureExpandedExAnimationRuntimeInstalled` checks that shared pointer/current JSL before descriptor
`$169` at logical `$02418`. A legacy `$02418` JSL names the obsolete `$140` auxiliary table, while
the runtime reached through `$0283AD` names the old `$600`/512-entry pointer table at `+$1A`.
Rust now distinguishes that generation, requires the runtime's exact RATS owner and all three
non-overlapping storage extents, installs the current runtime, converts every live old slot, erases
the obsolete tables, repairs the checksum, and collapses the staged result into one exact Undo.
`ConvertLegacyExAnimationRecords` (`0045E9C0`) gives the first fully bounded migration model. Each
legacy record is exactly `$23` bytes: one packed control byte, one destination word, and sixteen
source words. A zero low nibble is inactive. After decrementing the control byte, its high nibble
maps all sixteen old type classes into current kinds `$13/$0F/$01/$10/$11/$02/$03/$04/$05/$06/$07`,
while adjusted low nibbles 1–3 become the three two-word trigger forms. The converter detects the
smallest repeated 1/2/4/8/16-frame period (up to eight two-word frames) before serialization. Rust's
`convert_legacy_exanimation_records` now reproduces this mapping with the original 32-record clamp,
strict exact-length input, canonical current records, and exhaustive type/period/trigger tests.
Disassembly resolves the surrounding legacy storage framing as well. The old table is exactly
`$600` bytes (512 contiguous three-byte pointers). Presence tests only the pointer's bank byte. A
present pointer addresses one count byte; the migration masks it with `$3F`, clamps it to `$20`,
then reads exactly `count * $23` record bytes beginning at the following address. Rust's
`LegacyExAnimationRomLayout` and `load_legacy_exanimation`/`load_all_legacy_exanimations` implement
that complete read boundary, including exact table shape, all 512 slots, mapper conversion, bounded
payload reads, typed failures, and the original empty-pointer rule.

### Graphics 8×8 selected-tile edit buffer

`SelectGraphicsTileForPixelEditing` at `$00505120` copies the selected decoded tile into the
64-byte buffer at `$00ACF908` and records its index at `$00ACF900`. The X/Y/R branches in
`HandleGraphicsEditorCommand` at `$005054D0` and `PaintGraphicsTilePixelAtPoint` at `$00505380`
mutate only that private buffer. `CommitEditedGraphicsTile` at `$00504E00` is reached by the sheet
right-paste branch at `$00507A58`; it copies the buffer into an eligible decoded/backing tile.

The retained isolated-Wine `graphics-pixel-buffer/oracle.tsv` confirms that two horizontal flips
change and restore the edit buffer without changing planar backing. Painting diagnostic tile
`$600` changes buffer pixel zero from 0 to 1 while both decoded and planar backing remain exact.
This is deliberately distinct from the retained cache-paste oracle: high diagnostic tiles remain
selectable and paintable, but the paste predicate prevents `$600` or later from receiving the
staged buffer. Native pristine and installed editors therefore retain a selected edit tile across
painting, transforms, color mapping, copying, and F9; backing changes only when a permitted sheet
paste succeeds.

`CopyEditedGraphicsTileToClipboard` at `$005051E0` registers the singular format name
`Lunar Magic 8x8 Tile`, allocates exactly `$40` bytes, and copies the private edit buffer without a
header. `PasteEditedGraphicsTileFromClipboard` at `$005052B0` requires that registered format and
an allocation of at least `$40` bytes, then copies only the first `$40`. This single-tile format is
distinct from the general selector's plural `Lunar Magic 8x8 Tiles` rectangle format. The Windows
Rust frontend now publishes and consumes the exact single-tile record on all three graphics
surfaces while publishing its Unicode typed envelope in the same clipboard transaction for
portable interoperability. The bounded Rust decoder rejects color indexes outside 4bpp instead of
allowing malformed native clipboard bytes to reach palette rendering.

The retained isolated-Wine `graphics-single-tile-clipboard/oracle.tsv` binds those original entry
points to the registered cross-process boundary. A newly opened edit buffer copies as exactly 64
zero pixels. Publishing four repeats of indexed row `00 01 ... 0F`, invoking original paste, and
copying again returns all 64 pixels byte-for-byte, independently proving the headerless row-major
record consumed by Rust.

Shared/full palette transfer is retained across both original storage backends.
`ExportSharedPaletteFile` (`$239D`) and the palette editor's `ExportFullPaletteFile` control
`$2264` emit identical files: `$7E2` bytes in legacy mode and `$810` bytes when process backend
byte `DAT_00e27909` is one. The expanded writer appends the separate 16-byte auxiliary prefix to
the `$800`-byte main body. `ImportSharedPaletteFile` (`$239E`) requires at least `$400` bytes,
reads the active backend's exact size, and commits through `SaveSharedPalettesToRom(1)`. A live
legacy mutation reopens exactly and preserves the ROM checksum; controlled live exports cover
both original backend writers. Rust's reciprocal import/export gates cover both layouts,
legacy-to-expanded installation, reopen, and failure-atomic downgrade rejection.

The level-header persistence variant boundary is complete. One generated product exercises
SMW-J, SMW-NA, and All-Stars+World-NA; LoROM `$20`/`$30`, SA-1 `$23`, and ExLoROM `$32`; exact
copier-prefix absence/presence; and default/alternate allocation banks. Every case changes all
five legacy-header bytes through their semantic setters, including mode, screens, graphics,
palettes, music, preset time, and Layer 1 scroll, then adds the custom-time `$28` control and edits
the sprite header. The saved image passes identity/checksum detection, semantically reopens every
field, preserves the exact physical prefix, traverses byte-exact Undo/Redo, and is logically
identical between headered and headerless forms. Original gameplay assertions remain a separate
Oracle requirement rather than being inferred from this persistence product.

The graphics 8×8 editor's retained observations are now one executable isolated-Wine gate.
It opens `Window8x8`, reproduces the complete pixel-buffer and guarded cache-paste TSVs through
actual window messages, verifies the singular `Lunar Magic 8x8 Tile` clipboard allocation at
exactly `$40` bytes in both directions, and confirms no ROM byte changes. The controlled
diagnostic-page setup changes only maximum-page global `005E54F0`; every observed flip, paint,
selection, clipboard, accepted paste, and rejected paste then traverses the original window or
named clipboard entry point. The dedicated helpers accept either the historical numeric process
ID or an exact executable name so isolated automation cannot attach to another Lunar Magic
session.

`ConvertRomTo64MbitExLoROM` (`0047FCE0`) was revalidated against a modified 4 MiB source containing
standard 4bpp graphics and ExGFX. Lunar Magic writes metadata before relocation, performs the same
byte-exact layout transform already modeled by Rust, reloads the active descriptor, and writes the
final relocated metadata. The decisive compatibility invariant is the source pointer form:
Lunar Magic's standard split-plane table and shared GFX32/GFX33 operand publish LoROM's low-bank
mirror. A high-bank LoROM mirror is equivalent before conversion but selects the wrong physical
half in ExLoROM. Rust therefore models low-bank split-byte and shared-bank pointers explicitly and
applies them only to LoROM, retaining address bit 23 for ExLoROM and SA-1. A live conversion and
export gate now proves all 52 GFX files plus retained `ExGFX80` and newly inserted `ExGFX81`.

The standard-GFX installer now has an authentic SA-1 Pack variant as well. SA-1 Pack v1.40 was
applied to the retained pristine SMW-US ROM with its official BPS release, producing the
copier-headered 1 MiB source SHA-256
`926d28f2c8b0298b3b1744ac2d90c6e9a64260b7740eab5e195c0cbef38273c3`. Lunar Magic 3.63's
first modified-GFX insertion proves that most fixed 4bpp edits are shared, but the SA-1 `$7B`
graphics-buffer operands must remain `$7B`, the ordinary LoROM `$7E/$7F` RAM-reference rewrite
must not run, and the 32-byte DMA helper cannot occupy `$080000` because SA-1 Pack already owns
that RATS block. Lunar Magic instead allocates the helper at the first valid free RATS location
(`$084F4A` in this oracle) and patches `$0013F7` to its mapper-canonical payload address
`$10:CF52`. Rust now authenticates those SA-1-specific preconditions, performs the dynamic helper
allocation and mapper fixup transactionally, writes all 52 compressed files with SA-1 pointer
conversion, preserves the three pre-existing SA-1 Pack RATS owners, and reopens every file before
publication. The live Wine gate changes GFX00, installs the Rust result, and requires Lunar Magic
to export all 52 files byte-for-byte while retaining SA-1 identity and a valid checksum. ExGFX
first installation remains a separate mapper gate because Lunar Magic installs expanded-settings
storage and descriptor-routed extended tables at SA-1-specific locations.

An independently sourced historical optimized-LZ2 LoROM identifies a second authenticated runtime
generation at the fixed graphics hook. Its exact `$1AF`-byte RATS payload has CRC-32 `b5f7eda1`,
SHA-256 `7aaeae2444099f92a3f08406a92729cfaf5072e1988c9acc3dced1408ca5ee02`, and trailer
`LM 00 01`; the current LoROM optimized runtime remains the distinct `$1C0`/`LM 01 01` family.
Lunar Magic 3.63 converts the older ROM while also upgrading GFX17's legacy fourth plane. The
unchanged 54-file ExGFX export set shows that the editor understands the old ExGFX table even
though the current Rust table resolver does not. This establishes the next migration boundary as
the coupled legacy graphics-format and pointer-table transition, not merely another decompressor
payload replacement.

That coupled boundary is now modeled. The `$07F873` long operand resolves the historical `$6D00`
expanded-settings owner at logical `$0801E7`; its `$2D00` prefix is the relocated `$100..$FFF`
ExGFX table. This yields 21 live extended entries in addition to 33 ordinary ones. Two historical
streams decode to `$FFF` bytes, which Lunar Magic preserves exactly, establishing that conversion
must retain bounded pre-existing shapes rather than enforce only later import sizes. GFX17's only
conversion difference is plane 3 for tiles `$00/$01/$10/$11`: the 32 bytes at encoded offsets
`$011..$01F`, `$031..$03F`, `$211..$21F`, and `$231..$23F` change from `$00` to `$FF`.

The earlier overworld event loader is exact as well: primary-runtime offsets `$1D/$3C` are `$80`,
the index/reveal/state hooks use high LoROM banks `$85/$83/$83`, and reveal-runtime offset `$16`
is zero. Lunar Magic's LZ3 conversion retains these immutable bytes while replacing both stream
pointers. Rust recognizes both generations and migrates their owned streams. The complete Rust LZ3
result reopens in Lunar Magic without mutation and reproduces all 52 GFX and 54 ExGFX exports.
The reverse path is generation-stable rather than destructive: Lunar Magic's LZ3-to-LZ2 Orig and
LZ3-to-LZ2 Speed results retain the upgraded GFX17 and the historical settings/event families.
Rust does the same for both modes, and Lunar Magic subsequently treats each Rust output as a
no-op target and reproduces its complete 106-file graphics export set.

The bitmap clipboard normalizer has a distinct post-palette edge phase. A 17×16 source is aligned
to 32×16, but synthetic pixels do not all enter color reduction as an ordinary fill color.
Padding inside a partially covered 8×8 cell retains index zero. A wholly synthetic 8×8 cell is
then materialized through the Map16 editor's selected back-area palette entry; the vanilla/default
live state uses palette row 0, entry `$D`. Consequently that cell becomes a real graphics tile
(`$202` in the discriminating capture), not the configured blank tile `$0F8`, and the source color
sets and generated palette remain unchanged. Reapplying graphics materialization and Map16
construction after this synthetic-cell substitution matches the original palette, graphics, and
all 65,536 live Map16 definitions byte-for-byte.

## Dedicated overworld animation-options runtime

Lunar Magic 3.63 installs the per-map overworld animation-option storage through the dedicated
function at PE address `$004B2440`; it is not the expanded level-ExAnimation runtime at
`$0045CAF0`. The function concatenates executable ranges `$005C3040..$005C322F` (`$1F0` bytes),
`$005C3238..$005C3547` (`$310` bytes), and `$005C2918..$005C3037` (`$720` bytes) into one `$C20`
LoROM payload. Mapper-conditioned targets append `$20` bytes from `$005B5754`, but pristine
SMW-US LoROM takes neither that suffix nor any IRAM conversion branch.

The ordinary path allocates three independent RATS owners in order: the `$C20` runtime, a
`$15`-byte auxiliary initialized with `$FF` at every third byte, and the seven zeroed option
bytes. It installs JSL hooks at logical `$020086` and `$0024E3`, redirects the operand at
`$0200E0`, changes logical `$020102/$02010D/$02013B` from `$13` to `$14`, and publishes the option
owner through runtime `+$4A`. The relocation pass contains 25 explicit fixed/allocation pointer
sites plus 108 low-word sites at runtime `+$B3B`, whose source offsets are the low 15 bits of the
table at executable `$005C2F53`. The Rust installer embeds the exact three-fragment template,
applies this complete LoROM relocation network transactionally, and authenticates all immutable
runtime/auxiliary bytes and allocation owners on reopen while allowing the seven option bytes to
remain mutable.

## Descriptor-backed built-in overworld animation table

`LoadNativeGraphicsAndCoreTables` at `$004BA8D0` resolves the original overworld-animation source
table through active ROM-layout-descriptor field `+$5C4`, seeks that physical file offset, and
copies exactly `$86` bytes (67 little-endian words) into `$00CA7D08`. The ordinary SMW descriptors
at `$005E9DE8` and `$005EAA00` both store physical `$020200`; after removing the copier prefix this
is logical `$020000`. ExLoROM selection adds `$400000` before the read, choosing logical `$420000`
in the active upper SMW body rather than the lower compatibility mirror. The All-Stars + World
descriptor at `$005EB610` instead stores physical `$1A0200`, selecting its relocated SMW table at
logical `$1A0000`. An authentic SA-1 Pack conversion retains the ordinary table at `$020000`.

Rust publishes these three descriptor-equivalent layouts in `lm-profile` and the installed
overworld lifecycle loads only the identity-selected table. Every one of the 67 VRAM source words
must be in `$2000..$C7FF`; a truncated or malformed selected table aborts open and cannot fall back
to a plausible table in another mapper's location. The retained pristine table, synthetic
SMW/ExLoROM/SA-1/All-Stars routing, lower-mirror decoy, corruption, full overworld-raster reopen,
and exact built-in phase materialization gates cover the recovered boundary.

The same descriptor family resolves original overworld lightning. Field `+$904` names the
physical mask operand inside the selector routine; the 128-byte selector source begins one byte
earlier. Field `+$90C` names the eight delay bytes, followed immediately by eight initial colors.
The SMW-US descriptor yields logical delays/selectors `$0276F8/$027708`; SMW-J yields
`$0276F0/$027700`; and All-Stars + World yields `$1A76EC/$1A76FC`. ExLoROM adds `$400000` to the
selected SMW body, while an authentic SA-1 Pack conversion retains the SMW-US locations.

Rust authenticates the selected family with nonzero delays, color indexes 1–7, and the recovered
eight-byte selector prologue. Complete foreign or corrupt routines disable only built-in lightning;
truncated selected sources reject. No cross-identity or lower-mirror fallback is attempted.
