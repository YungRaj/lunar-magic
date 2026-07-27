# Lunar Magic reverse-engineering ledger

This ledger accompanies the live Ghidra database. Names are intentionally conservative.

Confidence levels:

- **High**: behavior is directly supported by decompilation, constants, strings, and/or xrefs.
- **Medium**: subsystem is clear but some parameters, flags, or edge cases remain unresolved.
- **Tentative**: useful working hypothesis; do not copy into a reimplementation without more evidence.

## Coverage

Latest measured internal Functions-table coverage: **4,026 named / 4,026 listed**, with **zero `FUN_...` placeholders remaining**. The earlier 3,912-function audit omitted an address-taken standard-object renderer cluster and 48 MSVC startup initializers that Ghidra had decoded as instructions without creating all required function bodies. Both clusters have now been promoted to real functions, named, prototyped, documented, and verified in the live Ghidra Functions table on port 8089. Ghidra's separate total-function count is **4,412**, including **386 imported/external symbols**.

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

The same pristine headered copy produced three additional exact compatibility corpora:

- `-ExportAllMap16` emitted a 651,760-byte `LM16` container. Its eight directory entries describe
  a `0x80000` combined tile bank, `0x10000` Acts Like bank, aliased `0x40000` foreground and
  background halves, an absent optional extended bank, and `0xF000`, `0x100`, and `0x40`
  auxiliary sections. The lossless `Lm16Map16File` Rust decoder preserves those intentional
  aliases and re-encodes the real file byte-for-byte.
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

The `00403a50`-`00407a40` batches recovered the level-mode/property dialog: entrance and completion actions, FG/BG indices and offsets, horizontal/vertical scroll modes, Layer 3 choices, music and tileset names, manual object/sprite command parsing, and level-mode table editing.

The screen-exit object boundary is now independently recovered across
`DeduplicateScreenExitObjectsByScreen` (`00437190`),
`BuildPackedScreenExitArrayFromObjects` (`0043acd0`), and
`SetScreenExitObjectForScreen` (`0043ad90`). Layer 1 command-zero records use parameter `0` for a
four-byte compact exit and parameter `2` for a five-byte extended exit. Byte 0's low five bits are
the source screen. The compact form stores the destination/flag high nibble in byte 1 and its low
byte in the first extension; the extended form stores the complete high byte in a second extension.
Lunar Magic keeps at most one exit per screen, selects the compact form when the destination's top
nibble is clear, upgrades to the extended form otherwise, and preserves byte 0's unrelated
new-screen bit. A reciprocal Wine import/re-export now confirms this interpretation on an actual
pristine-ROM screen-exit record.

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

The `00414bb0`–`004178b0` range identifies interface/VRAM-patch option tooltips, the About dialog and URL clipboard path, and the beginning of the overworld ExAnimation subsystem. `AboutDialogProc` has a recovered Win32 callback prototype. Overworld animation address/frame conversions, remapping, submap selection, slot display, and duplicate-trigger checks are named separately from their level-editor counterparts.

The overworld ExAnimation editor is now named through `OverworldAnimatedTilesDialogProc` at `004188d0`, including its frame-edit subclass, record commit, shift/rotate behavior, and tooltips. The following functions through `0041ab70` identify overworld submap options and both combo-based and edit-field variants of the Layer 3 graphics settings editor.

The `0041b410`–`0041e300` range identifies the analogous overworld foreground graphics editors, graphics-index list transfer, Overworld Options dialog, event reveal tile-pair editor, manual overworld sprite-command parser, and common error reporters. `OverworldOptionsDialogProc` has a recovered Win32 callback prototype. The 22-entry source/destination reveal arrays and their selected-row global are typed and named.

The `0041e3d0`–`00422260` range identifies filename/common-dialog helpers, in-place copier-header conversion, the Lunar IPS creation/application engines, ROM expansion/metadata loading, level-layout dimension tables, and the first core level-object tile renderers. IPS normal records, RLE records, reserved EOF handling, optional truncate metadata, sparse growth, header normalization, and logging behavior are annotated for clean-room reuse.

The `00422330`–`00424210` range consists of fixed and lookup-driven standard-object renderers. Names currently describe proven geometry and tile-selection behavior (single cells, horizontal/vertical pairs, 2x2, 3x3, 4x4, and composite patterns); comments explicitly mark exact in-game object identities as unproven where dispatcher evidence is still pending. The `0x3800`-cell Map16 tile/flag/source arrays and `0x4080`-entry modified-cell list are typed and named.

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

The custom object clipboard and template-placement subsystem through `0043b100` is now named. `LevelObjectClipboardHeader` is a recovered 32-byte structure followed by fixed 0x28-byte node records in registered format `Lunar Magic Objects V6`; copy computes selection-origin and rendered-margin metadata, while paste validates format/version fields, filters incompatible extended objects, converts encodings, translates clones, and reinserts them. The same node-stream decoder supports temporary multi-object templates, isolated preview rendering with full live-Map16 save/restore, and placement of cloned template groups. Screen-exit object evidence also upgraded the earlier generic per-screen control-node interpretation: dedicated helpers now deduplicate, create/delete, normalize flags, and synchronize the 32-entry packed screen-exit array.

The object-edit pipeline through `0043cb20` is now named and commented. It clones selections into template lists, converts connected Map16 regions into rectangular Direct Map16 object commands, performs handle-driven resizing with transition-safe detach/reinsert ordering, and rebuilds/redraws cached modified-cell regions. Three helpers isolate the packed properties and 15-bit reference-remapping behavior of encoded extended command ID `0x27`; the bit packing is established, while the user-facing meaning of that referenced resource remains intentionally marked medium confidence.

The following block now covers object-reference remapping scripts, manual-command insertion, invalid screen-exit diagnostics, legacy object-stream filtering, and the beginning of the Direct3D 9 renderer. The remapping language uses a 0x8000-entry translation table with replacement, signed-offset, sequential, and 16x16-grid modes. Renderer helpers dynamically resolve `Direct3DCreate9`, partition large render extents into device-compatible tiled surfaces, build four textured vertices per tile, and release COM resources along each failure and shutdown path.

The Direct3D and Windows compatibility range through `00440f90` is now classified. It includes complete renderer context lifecycle and tiled presentation, lazy USER32 multi-monitor API resolution with a single-monitor fallback, external ROM/GFX editor command-template expansion and process launch, executable-path startup validation, and CHM help-file opening with `Zone.Identifier` cleanup.

The ROM-address and level-coordinate utilities through `00441fe0` are now named and annotated. High-confidence helpers implement both directions of SNES/PC address conversion for the detected mapping mode, horizontal/vertical level-layout cell indexing, status/scrollbar initialization, and packed ROM-word access. Several register-convention stream wrappers are intentionally given structural names and medium-confidence comments until their hidden value widths can be proved from disassembly and all callers; the ExLoROM Work RAM bank-byte validator is separately identified by its required `0x7E`/`0x7F` values.

The range through `004431c0` now identifies expanded-ROM relocation validation, level dirty-state propagation, level-state teardown, packed level-header setters, writable-ROM-range checks, screen-count derivation, common file dialogs, auxiliary editor window lifecycle, and DPI-aware icon installation. The four compact level-header writers document exact byte and bit positions but deliberately leave the UI field names unresolved until the associated dialog controls or format tables prove whether each field is a palette, tileset, or other selector. Relocation helpers separately validate IRAM word/byte ranges and Work RAM bank bytes before altering ROM data.

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

The level-editor overlay and viewport-composition block through `004530a0` is now named. Low-level primitives cover clipped saturating color addition, blended double outlines, selection borders, dashed rectangles, Map16 grid lines, and clipped text with translucent background preservation. Higher-level passes draw logical screen boundaries and labels, screen-exit destinations, primary/midway and secondary entrance labels, level-mode boundary guides, and invalid Map16-cell warnings. `RenderLevelEditorViewportRegion` is identified as the main dirty-rectangle compositor: it initializes background pixels, renders the enabled level layers and sidecar graphics, performs priority redraws, and applies all configured editor overlays.

The full-level image export and initial SNES graphics-decoding block through `00455040` is now named. Export helpers calculate complete level dimensions, replace the interactive render surface temporarily, render the level in 16-pixel strips, and emit either bottom-up 24-bit BMP data or a packed RGB buffer passed to the PNG encoder before restoring editor state. The adjacent planar decoders expand SNES 8x8 tiles into indexed pixels, with a specialized 4-bpp implementation and a generic 1-through-8-bpp implementation.

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

The remaining staged Map16 hook installers and the adjacent sprite-19/Lfix3 patch family through `00471d50` are now named. The Map16 path now exposes four compatibility stages, auxiliary table allocation, final CDM16 repair, and mapper-specific compatibility upgrades. Embedded UI and patch labels prove the next cluster: it prompts for and installs Lunar Magic's sprite 19 ASM fix, recognizes Lfix3 runtime version `0x0110`, allocates the current 0x510-byte Lfix3 payload, initializes three 512-entry runtime tables, and migrates two legacy Lfix3 table layouts. Low-confidence names remain restricted to decompiler-elided signature probes and unidentified post-Lfix3 hooks.

Verified symbol coverage after this pass: **1,273 named functions out of 3,912 total; 2,639 autogenerated `FUN_...` symbols remain.**

Expanded secondary-exit support through `00473d30` is now named and structurally documented. The binary maintains six parallel 0x2000-byte planes for destination low, position/method, screen/Y, destination-high flags, X/overworld flags, and additional flags. The named functions detect legacy/current formats, allocate the current 0xD0-byte runtime plus four 0x200-byte tables, migrate packed flag bits, load all tables, locate/free/delete entries, and coordinate upgrades with Lfix3. A logical six-byte `SecondaryExitRecord` structure was added for clean-room reimplementation; comments note that the executable uses structure-of-arrays storage rather than interleaved records.

Verified symbol coverage after this pass: **1,302 named functions out of 3,912 total; 2,610 autogenerated `FUN_...` symbols remain.**

Secondary-exit serialization, the top-level ROM level loader/new-level initializer, known vanilla level-data fixups, and general palette file I/O through `004768c0` are now named. `SaveAllSecondaryExitTables` trims unused tails, preserves compact in-place planes, allocates variable-length planes, and updates mapped pointers; Save Level As retargets all incoming exits. `LoadLevelFromRom` now exposes the complete editor transaction from extension-table loading and mapper pointer conversion through object/sprite rebuild and redraw. Palette support distinguishes RGB `.pal`, versioned TPL, ZSNES/emulator state formats, raw SNES colors, and `.palm` selection masks, including byte-order inference and BGR555 conversion.

`ImportFullPaletteFile` and `ExportFullPaletteFile` establish the complete `.smwpal` layouts used by the clean-room model. The legacy palette backend transfers exactly `0x7E2` bytes. The expanded backend transfers `0x800` bytes from the main working-palette region followed by a distinct `0x10`-byte auxiliary region (located immediately before the main region in memory but appended after it in the file), for an exact `0x810`-byte artifact.

`LoadPaletteFromSupportedFile` and `SavePaletteToSupportedFile` prove the native TPL version-2 framing independently: ASCII `TPL`, one version byte equal to `2`, then exactly `0x200` bytes containing 256 little-endian SNES BGR555 words. TPL version `0` instead contains RGB triplets and remains a separately interpreted variant rather than being accepted by the native-word decoder.

The same dispatcher proves the extension-independent raw palette as exactly `0x202` bytes, or 257 little-endian SNES colors. Its optional sibling `.palm` file is exactly `0x101` selector bytes: a zero retains the working color and any nonzero value imports the corresponding source. After import, selected first colors of rows 0–15 (indices `0x00`, `0x10`, …, `0xF0`) are forced to zero; the separate color at index `0x100` is not part of that clearing loop.

RGB `.pal` files are exactly `0x300` bytes: 256 ordered red/green/blue triplets. `DetectPaletteRgbByteOrdering` is more precisely an expansion detector. For enabled colors, it counts evidence with any low-three channel bits and separately counts triplets whose low bits are all zero but whose high three bits are nonzero; a strict majority of the latter selects high-bits-only `xxxxx000`, otherwise five-bit values use replicated low bits. The conversion routine chooses the nearest replicated value for noncanonical inputs, preferring the higher five-bit level on an exact distance tie.

Verified symbol coverage after this pass: **1,317 named functions out of 3,912 total; 2,595 autogenerated `FUN_...` symbols remain.**

MWL level-file import/export and recent-file UI support through `004797d0` are now named. The importer auto-detects binary `LM` containers and legacy text manifests, validates versioned section offsets/sizes, upgrades historical headers and ExAnimation records, imports packed secondary exits, and converts stored SNES addresses for each mapper. The binary exporter writes MWL version `0x0363` with an eight-entry section directory covering level header, Layer 1, Layer 2, sprites, palette, secondary exits, ExAnimation, and the expanded header. Legacy export writes the text manifest plus `.mw0`-`.mw3` sidecars. Recent-file helpers manage ten paths, UTF-8-safe abbreviated menu labels, insertion/removal, and persistent menu rebuilding.

Two format structures were added: `MwlSectionDirectoryEntry` (8 bytes: file offset and byte length) and `MwlSecondaryExitEntry` (8 bytes: 16-bit exit index, five semantic field bytes, and one reserved byte).

Verified symbol coverage after this pass: **1,335 named functions out of 3,912 total; 2,577 autogenerated `FUN_...` symbols remain.**

MWL save orchestration and the level-editor undo/redo core through `0047b320` are now named. The save wrapper serializes Layer 1/2 and sprites before selecting binary or legacy export and updating recent files. Undo history uses fixed 0x28-byte doubly linked nodes with ownership/change flags; snapshots may share unchanged layer payloads and optionally include a 0xC00E-byte extended block containing fourteen header bytes plus all six 0x2000-byte secondary-exit planes. Capture, restore, pruning, allocation failure, reset, history-limit configuration, and menu-state updates are labeled. Adjacent helpers for copying a background from another level, reloading object/graphics/Layer 3 resources, finalizing edit transactions, and validated/fast redraws are also named.

The `LevelUndoRecord` structure was added at 0x28 bytes with flags, four snapshot pointers, three metadata words, and next/previous pointers.

Verified symbol coverage after this pass: **1,353 named functions out of 3,912 total; 2,559 autogenerated `FUN_...` symbols remain.**

Level-editor redraw dispatch, ROM-layout descriptor conversion, checksum compensation, Lunar Magic version detection, and the top-level ROM-open transaction through `0047d230` are now named. The ROM validator recognizes base SMW revisions and All-Stars+World, detects copier headers and LoROM/ExLoROM/SA-1 mapping, selects or converts the 0xC10-byte layout descriptor, verifies checksum/version compatibility, decodes installed runtime metadata, and initializes feature state. `ProcessRomImageOpenTransaction` then loads sidecars, Map16, secondary exits, ExAnimation, metadata, graphics tables, and the active level. The checksum routines implement mirrored SNES checksum accumulation for non-power-of-two images and write a compensation block when needed.

Verified symbol coverage after this pass: **1,371 named functions out of 3,912 total; 2,541 autogenerated `FUN_...` symbols remain.**

ROM expansion/metadata and graphics-compression management through `00480760` are now named. Lunar Magic's metadata writer emits the public-version identification/attribution block, packed mapper and feature flags, compression configuration, runtime pointers, VRAM version, and optional checksum compensation. Graphics helpers resolve AllGFX offsets, recognize stock signature pairs, synthesize a missing fourth SNES bitplane for compatible assets, erase standard/auxiliary/ExGFX allocations, and report insertion failures. The compression-mode transaction extracts graphics when formats are incompatible, replaces mapper-specific mode-1/mode-2 runtime resources, converts dependent tables, reinserts graphics, updates metadata, and cleans its temporary directories. The ExAnimation `Bypass.lst` exporter is also labeled.

Verified symbol coverage after this pass: **1,389 named functions out of 3,912 total; 2,523 autogenerated `FUN_...` symbols remain.**

Legacy ExGFX bypass import, Layer 3 tilemap GFX writing, level-mode Layer 2 classification, and the five per-level payload pointer-table loaders through `004814a0` are now named. `ImportLegacyExGfxBypassList` installs ExAnimation feature control support when necessary and commits the 0x400-byte `Bypass.lst` table. The pointer loaders read up to 0x209 three-byte entries for Layer 1 objects, Layer 2, sprites, optional palettes, and ExAnimation, handle multiple installed table generations/sentinels, convert SNES addresses to mapper-specific PC offsets, and zero-fill unused indices.

Verified symbol coverage after this pass: **1,399 named functions out of 3,912 total; 2,513 autogenerated `FUN_...` symbols remain.**

The graphics and level-payload cleanup block from `00481700` through `00482f00` is now named and annotated. It includes standard GFX, ExAnimation GFX, ExGFX, and special-GFX PC-offset table loaders; payload-span clamping and object/compressed-stream measurement; safe unlink-and-erase helpers for Layer 1, Layer 2, palette, sprite, and ExAnimation data; and deleted-level settings reset. The cleanup routines build a five-table reference index with the current level excluded, preventing shared ROM payloads from being erased.

Verified symbol coverage after this pass: **1,414 named functions out of 3,912 total; 2,498 autogenerated `FUN_...` symbols remain.**

The level-save-adjacent workflow through `00487820` is now named and annotated. This pass identifies internal-emulator sprite serialization, bulk graphics-pointer rewriting and integrity-word updates, ROM level-access restriction, directory-wide MWL insertion and export, rendered level-image export, bitmap-driven level deletion, hexadecimal level-list parsing, usage-report generation, and migration followed by clearing of the original SMW level-data area. The deletion coordinator explicitly protects shared Layer 1, Layer 2, sprite, palette, and ExAnimation payloads before resetting settings and secondary exits.

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

The 256-byte `RestoreDirectoryRecord` type is now applied to its live global buffer at `00931b48`. The restore archive header storage and all four seven-byte ExAnimation option arrays are also typed, renamed, and plate-commented in the listing.

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
storage and bit 2 selects the newer split-plane layout. Native remapping promotes the descriptor
to compressed split-plane storage, records the result bank, and clears the legacy/direct-pointer
bit. The Rust MWL boundary now exposes a typed descriptor while retaining every unknown bit and
the opaque second source-address word. All 525 retained MWL files were surveyed: 499 carry
descriptor `$0000000C`, 26 carry `$00000000`, and none supplies cross-bank fixture coverage.

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

The lowest-address residual-symbol audit and the UI/color-quantization tail through `0057ed30` are now named and annotated. This pass recovered all remaining LZ2 back-reference/fill primitives and its bounded 64 KiB decoder, relative level-object tile-list rendering, contiguous object-list snapshots, Direct3D adapter/device/reset/upload helpers, embedded level-editor graphics resources, Map16 and 8x8-viewer selection/grid rendering, overworld zoom adjustment, the complete Add Sprites dialog and tooltip infrastructure, and the variance-based Wu RGB quantizer used to emit rounded SNES BGR555 palettes. Latest verified coverage is **3,771 named functions out of 3,912 total; 141 autogenerated symbols remain.**

The statically linked CRT tail through `00589416` is now semantically separated from application code and annotated. Recovered library behavior includes HTML Help delay loading, secure/unbounded fread and fwrite cores, FILE and descriptor locking/unwind thunks, fclose and recalloc, ANSI/wide stat and mkdir, Win32 error translation, CRT exit and runtime-error paths, printf formatting, pointer encoding, per-thread data creation, text/Unicode descriptor reads, time conversion, ANSI stream-mode parsing, FILE allocation, SEH local unwind, and CRT signal dispatch. Latest verified coverage is **3,858 named functions out of 3,912 total; 54 autogenerated symbols remain.**

The final CRT/math tail is now named and annotated. It covers on-exit registration and terminate handling, CRT error-mode and message-box dispatch, multibyte/code-page initialization, timezone/environment storage, ANSI and wide descriptor-open cores, process-environment mutation, x87/SSE2 `pow` implementations and exceptional cases, x87 math-exception forwarding, and decimal-string/long-double conversion. This completed the original 3,912-entry symbol pass; the later function-boundary audit at the top of this ledger supersedes that historical count.

## Final symbol audit

- Live program: `Lunar Magic.exe` in the Ghidra session exposed on TCP port 8089.
- Internal functions enumerated: 4,026; imported/external functions: 386; Ghidra total: 4,412.
- Functions retaining a `FUN_...` name: 0.
- Every formerly autogenerated function received a semantic symbol and a decompiler comment recording behavior and confidence.
- Application code, compiler-generated cleanup/thunk entries, and statically linked CRT/math routines are named distinctly so clean-room reimplementation work can separate product behavior from library/runtime behavior.
- Recovered application data types and globals include level object/sprite nodes, multiple native clipboard headers, overworld endpoint/message/undo records, Map16 remap nodes, palette history records, sidecar streams, and editor backing buffers. Exact layouts are used where field offsets and sizes were proven; uncertain fields remain explicitly marked rather than guessed.

The adjacent overworld text pipelines are now separated and annotated as level-name, message-box, and boss-sequence storage. Expanded message text supports three pointer-addressed banks; boss-sequence text uses 56 fixed records. The overworld sprite subsystem is also named through its top-level load/save orchestration, including the seven-map custom-sprite stream, 24-record per-map limit, variable record-size table, built-in sprite tables, and ROM allocation lifecycle. Top-level overworld load/save and selective text-save entry points now have visible semantic symbols in Ghidra's Functions window. The following palette and expanded layer-tilemap initialization routines are named as well.

Title-screen, credits, and overworld Layer 3 graphics paths are now distinguished. This includes title-screen saving, title and credits graphics loading, legacy and expanded credits-row decoding, credits tilemap deduplication/serialization, and overworld Layer 3 graphics/tilemap loading.

The LMSW emulator-plugin integration block through `004c2ed0` is fully named and annotated. Recovered behavior includes DLL export resolution (decorated and undecorated APIs), lifecycle management, ROM and sprite transfer, pause-reason aggregation, single-frame stepping, editor scrolling, viewport backing-store capture/restore, overlay rendering, and level-load notifications. Added the 16-byte `LmswViewportRect` structure, applied it to capture/restore prototypes, recovered several scalar prototypes, and named the principal LMSW state globals and drawing/pause/step export pointers.

The following level-editor sprite rendering and manipulation subsystem through `004ce5e0` is now substantially annotated. Recovered the per-cell linked tile renderer, signed-offset and screen-wrapping logic, 256-entry standard-sprite render dispatch table, custom metadata rendering path, entrance rendering and packed entrance-table synchronization, sprite stream parser/serializer, list sorting and insertion, selection deletion, dirty-cell invalidation, and group movement/clamping.

The complete byte-sized standard-sprite preview domain is now classified against that dispatch
table. IDs `$29`, `$30`, `$EE`, `$F0`, and `$F1` deliberately select Lunar Magic's native
empty/default handler; IDs `$F6`–`$FF` are reserved for SSC custom-display bookkeeping; every other
ID selects recovered built-in artwork. The Rust renderer exhaustively tests all 256 IDs against
this partition, and the native editor leaves intentional empty handlers artwork-free while
retaining a visible diagnostic when required custom-display data cannot be resolved.

The entrance synchronization path is now tied to the binary MWL boundary. `SynchronizeEntranceNodeData` (`004ccda0`) projects 40-byte editor nodes from packed main, midway, and secondary-exit state; `RebuildLevelEntranceNodes` (`004cd7e0`) creates one main node, a conditional midway node, and secondary nodes targeting the current level. `ExportBinaryMwlLevelFile` proves that the 64-byte level-header section owns main-entrance bytes at offsets `2`-`6`, `14`, and `15`, and midway-specific bytes at offsets `9`-`12`. The Rust `MwlLevelHeaderSection` exposes these as lossless typed records and the native MWL editor can modify them without normalizing the other 53 bytes. A reciprocal Wine oracle proves Lunar Magic 3.63 imports and re-exports a changed main position exactly; it also proves that midway-only bytes are normalized to zero when the destination ROM lacks Lunar Magic's separate-midway runtime.

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

Unicode-compatible Win32 wrappers are additionally named through `004db810`: combo-box text retrieval, UTF-8 virtual-key lookup, UTF-8 activation-context creation, window/dialog text setters and getters, dynamic preferred-UI-language APIs, the legacy language-ID fallback table, and UTF-8 preferred-language multi-string production.

The UTF-8 Win32 adaptation layer is now named through `004e0680`. It covers ShellExecute and CreateProcess, common open/save dialogs, UTF-8 file creation/deletion/copying and attributes, current/short/full/module/executable paths, drag-and-drop filenames, file enumeration, and UTF-8 LoadLibrary variants. Added the 1,140-byte `Win32FindDataUtf8` structure with 1,040-byte primary and 56-byte alternate UTF-8 filename buffers, and applied it to the first/next enumeration prototypes. The entry at `004ddd6a` overlaps the short-path routine and remains explicitly marked as a medium-confidence boundary.

The remaining UTF-8 compatibility wrappers through `004e4b30` are now named: image loading, command-line tail extraction, UTF-8 `fopen`, directory creation and file status, UTF-8 registry strings including multi-strings, menu item access, text measurement/drawing, text-bearing window messages, and HTML Help with a short-path fallback. Analysis then enters Lunar Magic graphics code: cached graphics-context cleanup, dialog hexadecimal helpers, decoding 1,024 SNES 4bpp planar tiles into chunky pixel indices, and initialization of a 1,024-entry identity tile-remap table. The remap array, initialization flag, and cache-state scalar are typed and named.

The Map16 file and editor subsystem through `004e7780` is now named and annotated. Recovered file formats include per-page `Map16Page.bin`/`Map16PageG.bin`, complete foreground `Map16FG.bin`/`Map16FGG.bin`, complete background `Map16BG.bin`, and the 0x1C000-byte sprite `.s16` sidecar. The SNES-file importer consumes a 0x8000-byte 4bpp graphics set and 0x800-byte screen map, optionally imports a palette, remaps tile numbers, deduplicates 16x16 definitions, and installs them into blank Map16 slots.

`LoadM16Map16SidecarData` reads one fixed `0x2000`-byte `.m16` block. The `.s16` loader first
zeros its full `0x1C000`-byte buffer and then reads any available prefix up to that capacity.
`WriteS16SpriteMap16SidecarFile` scans the buffer as `0x7000` little-endian dwords from the end,
keeps through the last nonzero entry, rounds the byte length upward to an `0x800` boundary, and
writes a minimum `0x800` bytes for the all-zero case. The raw dword fields remain intentionally
uninterpreted until their consumers provide stronger evidence.

Map16 rendering is traced from individual flipped 8x8 SNES tile descriptors through the cached 256x256 page bitmap and selected-tile previews. Clipboard copy/paste uses the custom `Lunar Magic 16x16 Tile` format; added the exact 10-byte `Map16TileClipboardRecord` containing four subtile descriptors and an acts-like value. The main editor window procedure, keyboard/control handlers, page navigation, import/export shortcuts, attribute flipping, acts-like-cycle detection, selection/paste paths, and cache lifecycle are named. The current page, selected subtiles, acts-like value, selected absolute tile index, and active-selection flag are typed and named.

The separate Map16 tile-selector/viewer subsystem through `004e99c0` is now named. It consists of an outer selector window, a scrollable 256x256 tile-view child, and a status bar. Recovered behavior includes DPI-aware percentage scaling, client/outer size calculation, horizontal and vertical scroll state, mouse-wheel page motion, hover and primary/secondary selection highlighting, keyboard page navigation, foreground-page unlocking, palette-context changes, and top-down 32-bit DIB cache creation/rendering/cleanup. Typed and named the current/maximum selector page, selected and hovered absolute tile numbers, palette context, and backing pixel pointer.

The outer Layer 1 selector creator and the beginning of the main level-editor presentation layer through `004eac40` are now named. This includes renderer/file-error reporting, status-bar sizing and DPI handling, horizontal/vertical level-editor scroll state, backing-cache and auxiliary-buffer cleanup, and the toolbar icon system. The toolbar uses a 24-entry command table with parallel enabled/disabled icon arrays, supports an external `Lunar Magic.ff5` bitmap, compressed and built-in fallbacks, per-window DPI scaling, right-to-left mirroring, and a separately rebuilt alternate mode cache.

The level-editor modification and selected-tile transaction layer through `004ebb10` is now annotated. The dirty-state setter drives command `0x2261` and save/discard/cancel prompting. Tile selection uses four 0x13D00-entry planes of per-tile state, rectangle rasterization, cached bounds/counts, temporary Map16 definition and acts-like snapshots, bounded translation, drag updates, and placement at a requested grid point. Live and temporary definitions are swapped so overwritten tiles remain recoverable during movement. Typed and named both 324,608-byte selection-state arrays, selected-tile count, nonempty-bounds flag, and drag-active guard. Three following capacity counters are intentionally named by proven mechanics because their exact resource-table identities are not yet established by callers.

The following palette-allocation engine through `004ece10` is now named. It initializes palette-entry reservation states, optionally propagates a selected color across eight rows, converts RGB to Windows HSL-240 coordinates, builds unique color histograms for 8x8 tiles, performs weighted RGB555 palette selection, and models recurring tile color sets and their subset dependencies. Added the exact 184-byte `PaletteColorSetRecord`, containing up to 16 colors, direct and aggregate weights, source pointers, subset pointers, selection flags, and aggregate total. The final greedy selector maximizes overlap with already chosen colors and aggregate utility while respecting remaining palette capacity.

The bitmap-import orchestration block through `004ef770` is now named and annotated. It extends weighted color sets into available palette rows, marks subset records assigned, maps imported pixels to palette indexes, detects blank and duplicate 8x8 graphics including horizontal and vertical flip equivalents, allocates free graphics slots, assembles or deduplicates 16x16 Map16 entries, commits editable palette changes, and drives the complete bitmap quantization/import pipeline. The occupancy scanner covers all `0x300` graphics slots; tile-map results preserve palette, priority, and flip attributes in the final 16-bit tile words.

The bitmap-import palette editor and dual-preview UI through `004f3370` is now named and annotated. The options dialog manages quantizer selection, palette-row reservations, fixed colors, color-count limits, priority, and snapshot restoration. Two custom child windows render the original and converted bitmaps with shared DPI-aware sizing and synchronized scroll positions; creation, painting, scrollbar configuration, resizing, and teardown are labeled separately.

The following controller and selection tooling through `004f5990` is now named. This includes the import-preview zoom menu and keyboard hook, the top-level bitmap import workflow, a textual remapping language that can transform graphics indexes, palette rows, Map16 indexes, and secondary-map values, and the custom registered `Lunar Magic 16x16 Tiles` clipboard serializer. Added the exact 0xA0-byte `LunarMagicTileClipboardHeader` with section offsets, selected count, rectangular dimensions, source Map16 index, flags, and explicitly represented reserved regions.

Map16 import/export, history, and visible rendering through `004f9e40` are now named and annotated. Added exact 64-byte `Lm16Map16FileHeader` and `Lm16Map16SectionDirectory` structures for the structured `.map16` format. Added the exact 811,788-byte `Map16UndoSnapshot` and typed its live linked-list globals. Rendering names now distinguish decoded tile composition, Acts Like overlays, selected-tile highlighting, page frames and labels, page boundaries, and bounded versus drag-selection marching ants.

Map16 interaction code through `004fd510` is now named and annotated. This covers tracking tooltips (including disabled controls and internal sprite/background tile descriptions), independent 100-5000 percent zoom with forward and inverse coordinate transforms, DPI-aware selector sizing, hover and auto-scroll behavior, selection creation and movement, temporary-buffer restoration, property-panel mixed-value analysis, priority and palette edits, horizontal and vertical flips, and Acts Like cycle detection.

Map16 Acts Like editing and the full 8x8-subtile interaction path through `004ff170` are now named. The recovered behavior includes cycle-rejecting Acts Like assignment, per-corner graphics-index edits, additive/subtractive 324,608-byte selection masks, clamped selection translation, live/temporary buffer swapping, 8x8-selector paste integration, hover and auto-scroll logic, and serialization/deserialization. Added the exact 0xA0-byte `LunarMagicSubtileClipboardHeader` with tile and auxiliary section offsets, selection dimensions, count, source index, flags, and reserved regions.

Native `Lunar Magic Map8 Tiles` copy/paste entry points and fallbacks are now named through `00500850`, including conversion from graphics-selector indexes, legacy BG Tiles v3 handling, raw subtile rectangle placement, and extraction/publishing helpers. The Map16 render child procedure is also identified in full, together with its key-command table dispatcher, character shortcuts, 256x256 32-bit DIB allocation, `.m16` fixed sidecar output, and block-rounded `.s16` sprite sidecar output.

The Map16 modeless parent dialog is now fully named through `00503ae0`, including its command dispatcher, exhaustive control initialization, DPI resize paths, render-child creation, teardown, and show/close entry points. The adjacent graphics editor is named through `00504fe0`: status-bar lifecycle, sixteen-by-sixteen color-map filter editing, indexed-to-SNES-4bpp encoding, active palette selection, editable 8x8 pixel grid, foreground/background swatches, complete tile-sheet rendering, commit propagation, zoomed presentation, and refresh paths.

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

### Pristine graphics pointer planes

`ReadGraphicsFileRomPointer` (`00463A90`) proves that pristine GFX files
`$00..$31` do not use the contiguous 24-bit pointer table assumed by the
expanded profile backend. Descriptor entry `$2A` (`+$A8`) supplies headered
base `$003B92`, hence logical base `$003992`. Lunar Magic reads the pointer's
low, high, and bank bytes from three parallel 50-byte planes at logical
`$003992`, `$0039C4`, and `$0039F6`. Entries `$32` and `$33` use the separate
packed-pointer operands at descriptor entries `$2C`, `$2D`, and `$2B`.
Expanded GFX/ExGFX ranges use still other descriptor-selected tables.

This distinction is now the primary automatic-profile boundary for pristine
SMW US revision 0. The Rust layout model must represent split three-plane
graphics pointers explicitly; treating `$003992` as a contiguous table would
silently combine bytes belonging to different files. Native UI auto-detection
must therefore select the split-plane backend until a verified expanded
graphics runtime is installed.

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
`$2000`-byte workspace boundary. `InsertLayer3TilemapGraphicsFile` (`004690E0`) consumes the same
fields and tables. The main Layer 3 patch is an exact `$4C0` allocation, while the expanded-settings
runtime/table is an exact `$6E00` allocation whose table payload is reached through descriptor entry
`$70` at runtime offset `$1C0`.

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
continuation entry `+$578` to `$0021DA`, and allocation search entry `+$C4` to `$06ABF7`.
The pristine 17-byte hook begins `AE F4 1D`; installation replaces it with a `JSL` and fixed
continuation tail. Its RATS-owned `$60`-byte runtime begins `08 C2 20` and refers to the separately
owned movement payload at biased addresses `payload+2`, `payload-3`, and `payload-2`.

The Rust implementation validates every fixed hook/runtime byte, both owners, the continuation
word, and agreement among all biased pointers. First installation allocates both blocks; updates
retain the proven runtime and reclaim only its proven recording owner. Allocation, pointer
publication, checksum repair, semantic reopen, and history commit are failure-atomic. `lm-title`
separates movement/container parsing from ROM mutation, while CLI and application workflows expose
native, ZSNES, and Snes9x import/export without platform APIs.

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

## Native `-TransferOverworld` Map16 allocations

Tracing `SaveAllOverworldEditorData` (`004BF550`) through the live 3.63
decompilation resolves the first allocations in the pristine-US Wine oracle.
They precede the overworld-specific event/text allocations because the native
operation saves Map16 support tables in the same transaction:

- Payload `0x80008`, length `0x2F28`: the 16 KiB Map16 definition table. Lunar
  Magic splits even and odd bytes into two 8 KiB planes, sized-RLE encodes the
  planes back-to-back, and interleaves them on load. The fixture's planes
  consume 6514 and 5558 encoded bytes. Rust exposes this as
  `decode_interleaved_sized_rle_prefix`.
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

This corrects the provisional hypothesis that these four large owners held
overworld layer, palette, sprite, or ExAnimation data. They are Map16
definitions, acts-like data, and remap metadata installed by the top-level
overworld save. `LoadMap16RemapRangeGroups` (`004B6750`) validates both words
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
with the current packed position byte, level mode, and orientation, fits its complete composite
geometry into a preview cell, and labels empty/default handlers explicitly. Selecting an entry
constructs the proven `yyyyEESY / XXXXssss / NNNNNNNN` record, retains the form's position and
extra bits, and arms the same transactional canvas placement path.
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
decoded 1,024-word plane and paints it before both object layers.
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
