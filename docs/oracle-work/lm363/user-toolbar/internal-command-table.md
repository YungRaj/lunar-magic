# Lunar Magic 3.63 user-toolbar internal command table

## Authority

- Executable: `lm363/Lunar Magic.exe`
- SHA-256: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- PE image base: `$00400000`
- Ghidra loader: `LoadUserToolbarConfiguration` at `$004D4A50`
- Keyword lookup: `FindUserToolbarKeywordIndex`

The loader passes `$005E6A70` as the keyword-pointer base and bounds the search to `$013E`
(318) slots. It stores the selected original command ID from the parallel 16-bit table at
`$005E6F70`. In the executable's `.data` section these addresses map directly to raw file offsets
`$001E6A70` and `$001E6F70`.

The retained source fixture is
[`user_toolbar_internal_commands.tsv`](../../../../crates/lm-app/src/user_toolbar_internal_commands.tsv).
It records table slot, hexadecimal command ID, and keyword in original order. Slots `$000` through
`$13C` contain 317 named entries. Slot `$13D` is the null-pointer/zero-ID sentinel, so it is retained
as evidence but is not exposed as a command. `LM_EDIT_SELECT_ALL` intentionally occurs twice at
slots 71 and 90 with the same `$245D` ID. `LM_KEY_ADD_CSPRITE` and `LM_KEY_ADD_CUSTOM` are distinct
names sharing `$26AF`.

The table was extracted without executing Lunar Magic:

```sh
ruby -e 'b=File.binread(ARGV[0]); 318.times{|i| p=b.byteslice(0x1e6a70+i*4,4).unpack1("V"); s=""; if p != 0; o=p-0x400000; z=b.index("\0",o); s=b.byteslice(o,z-o); end; id=b.byteslice(0x1e6f70+i*2,2).unpack1("v"); printf("%03d\t%04X\t%s\n",i,id,s)}' 'lm363/Lunar Magic.exe'
```

## Rust boundary

`lm-app` exposes a typed lookup over the authenticated inventory. Tests require all 318 physical
slots, all 317 named entries in their original order, the terminal sentinel, duplicate-name and
shared-ID behavior, and successful `usertoolbar.txt` parsing of every named entry. `lm-native`
rejects invented internal names before dispatch, distinguishes an authenticated but not-yet-routed
command by its original ID, and currently routes 226 authenticated table entries to native commands,
level-view actions, or the same native workflow used by the corresponding menu. The direct workflow
set includes Help Contents/About, level analysis, restore-point creation/restoration, IPS
creation/application, the authenticated Sprite 19 installer, the integrated object/sprite placers
and their legacy aliases, return-to-level routing, integrated Layer 1/Layer 2/sprite editing modes,
selection-wide Select All/Delete/Delete All/Escape actions, typed multi-record Cut/Copy/Paste
for Layer 1 objects, object-backed Layer 2, and sprites, and all six mapper-gated ROM-expansion
commands: ordinary 2/3/4 MiB, warned 8 MiB ExLoROM conversion, and warned SA-1 6/8 MiB. The
authenticated `$23A3` `LM_FILE_EXPORT_DIRECTORY` route also opens the same all-level MWL batch
exporter used by the native Editors menu. `LM_MOUSE_LEVEL_BACK` and `LM_MOUSE_LEVEL_FORWARD` share
the same bounded navigation-history actions as their authenticated File-command counterparts.
The authenticated ordinary `LM_FILE_EXTRACT_GFX`/`LM_FILE_EXTRACT_EXGFX` and quick
`LM_FILE_EXTRACT_GFX_BUTTON`/`LM_FILE_EXTRACT_EXGFX_BUTTON` handlers reuse the atomic batch worker
without an ownership chooser and target the ROM-sibling `Graphics` (`AllGFX.bin` when enabled) and
`ExGraphics` paths. Ordinary commands show the recovered success resource after publication; quick
commands suppress only that completion presentation.
The authenticated quick insertion handlers `LM_FILE_INSERT_GFX_BUTTON` and
`LM_FILE_INSERT_EXGFX_BUTTON` use those same fixed sibling paths, retain the conditional 4bpp
format warning, validate in a cancellable worker, and publish one revision-bound atomic ROM commit.
Their ordinary counterparts remain distinct because original resources `$03EC` and `$03FE` expose
PC address, expansion, and 3bpp/4bpp ASM choices before insertion.
Command enumeration rejects false acceptance, but does not claim that the remaining commands are
implemented.

The native routes also preserve the editor destination of three level-menu commands:
`LM_LEVEL_GRAPHICS` opens the current level's 8×8 graphics editor, `LM_LEVEL_EXTEND_ANI` opens its
ExAnimation editor, and `LM_LEVEL_LAYER3_SETTINGS` opens its Layer 3 editor. Eight view/level
commands target the matching integrated built-in tool section instead of spawning a duplicate
window: background/Layer 2, sprite data, or level/entrance settings. Each activation restores the
fixed-width tool column and gives the requested section a fresh persistent collapse identity, so it
reopens even after the user previously closed it without resetting unrelated sections.

The four entrance-view commands now preserve the original renderer-state model. Ghidra
`HandleLevelEditorCommand` at `$00496000` shows commands `$23F8`, `$23F9`, and `$23FA` toggling
`DAT_005e7b0f`, `DAT_005e7b10`, and `DAT_005e7b11` independently before rebuilding the level
sprites. Command `$2414` toggles its own aggregate byte `DAT_005e7b0e`, copies that value into all
three renderer bytes, and synchronizes all four external-toolbar checks. Xrefs bind those renderer
bytes respectively to the primary, secondary, and midway paths in
`RenderLevelEditorViewportRegion` (`$004530A0`), `DrawSecondaryEntranceLabels` (`$00452D10`), and
`DrawPrimaryOrMidwayEntranceLabel` (`$00452920`). Rust retains the independent aggregate state,
draws only screen-exit-referenced secondary slots targeting the current level, preserves the
vanilla `$100` destination bit derived from slots `$100..$1FF`, and suppresses a separate midway
node when it overlaps the primary entrance.

The shared-palette command pair is also direct rather than a generic editor alias. Command `$239D`
(`LM_FILE_EXTRACT_PALETTE`) invokes the recovered `ExportSharedPaletteFile` path, while `$239E`
(`LM_FILE_INSERT_PALETTE`) invokes `ImportSharedPaletteFile`. Both now open or reuse the native
shared/custom-palette workspace and immediately start its exact `.smwpal` chooser. The transfer
retains Lunar Magic's `$7E2` legacy and `$810` expanded layouts, refuses stale revisions or a
concurrent transfer, and applies imported data through the existing staged checksum/Undo commit
boundary. The distinct current-level pair is now direct as well. `$239F`
(`LM_FILE_EXPORT_PALETTE`) composes an untouched vanilla level into Lunar Magic's exact 257-word
working buffer, or reads a nonempty installed custom slot, then publishes raw, TPL v2, or RGB24.
The three pristine level `$105` exports match the retained original hashes; TPL/RGB substitute
word 256 for every unsupported row-zero entry and RGB defaults to high-bit expansion. `$23A0`
(`LM_FILE_IMPORT_PALETTE`) discovers an optional same-name `.palmask`, applies the format-specific
working-buffer mapping, installs the custom-palette runtime when absent, writes the current slot,
repairs the checksum, and verifies an exact reopen before crossing the revision-checked mutation
boundary.

The bitmap pair now enters the same installed full-level renderer used by the native editor without
requiring a palette-ownership sidecar. `$23BD` (`LM_FILE_EXPORT_BITMAP`) captures the current
level with automatic screen extent and publishes an original-format Windows BMP. `$23BC`
(`LM_FILE_EXPORT_DIRECTORY_BITMAP`) starts the cancellable all-slot directory worker in its
recovered default state: only expanded-area levels, stored screen extents, uppercase three-digit
slot suffixes, and failure-atomic group publication. `$2393` (`LM_FILE_SAVE_DIRECTORY`) now routes
to the distinct modified-level MWL batch mode; `$2392` continues to export all 512 level slots.
Both retain the bounded worker, cancellation, and all-or-rollback directory publication boundary.
`$2525` (`LM_LEVEL_ENTRANCE2`) now directly opens or reuses the native Modify Secondary Entrances
workspace. It therefore reaches the complete 8,192-entry table and all six original packed planes,
with the existing detected-layout, staged-edit, revision, checksum, Undo, save, and reopen boundary;
it is not reduced to the unrelated current-level main-entrance section.
`$2523` (`LM_LEVEL_EXITS`) opens the integrated complete 32-screen staged form. Its one-batch
publication matches the recovered last-duplicate-wins normalization, absent-entry deletion,
advance-bit retention, compact/extended encoding selection, ascending list-end placement, and one
Undo boundary. Cursor-cell command `$26FF` now resolves the live pointer through the painted canvas
geometry, zoom, bounds, and orientation, then opens that table with the exact source screen
preselected. `$26FE` now resolves direct, midway, and secondary destinations, rejects absent and
overworld exits, and preserves the original dirty Save/Discard/Cancel gate; Save navigates only
after successful commit and any required expansion, while stale/failing revisions cancel the
deferred destination.
Level command `$2526` now performs the recovered undefined-exit scan over staged Layer 1 and
object-backed Layer 2 Map16 caches. It follows Acts-Like chains, includes the built-in pipe/door
set plus loaded `.dsc` exit markers, resolves direct and full-table secondary destinations, skips
overworld destinations, and reports the exact affected screen numbers for destinations `$000` and
`$100`.
Ghidra cases `$BA/$BB/$BC` bind level ExAnimation, global ExAnimation, and ExAnimation settings to
commands `$2530/$2531/$2532`. All three now open the installed-ROM ExAnimation workspace directly
from the current level without changing the primary editor mode. The level/settings routes start
on the level domain; the global route starts on the global domain. Repeated routes safely reuse or
switch a clean matching workspace, while a dirty or different-level workspace is preserved.
The authenticated partition is therefore 239 routed and 78 pending slots.
