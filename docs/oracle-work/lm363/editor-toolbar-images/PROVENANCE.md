# Lunar Magic 3.63 editor toolbar and tiled-image evidence

This fixture records metadata and command mappings only. It does not redistribute the original
bitmap resources or CHM topic bodies.

## Sources

- `lm363/Lunar Magic.exe`: SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- `lm363/Lunar Magic.chm`: SHA-256
  `6ff2a44ff32902aed11d1969970e2c19a91ef336c29795fed823b78e577d60be`.
- The labeled local Ghidra project `Lunar Magic.gpr`, opened without reanalysis and queried through
  the project bridge.

The authenticated CHM topic `html/info_custom_toolbar.htm` specifies these exact horizontal square
cell strips: `.ff1` 6, `.ff2` 32, `.ff3` 12, `.ff4` 41, `.ff5` 20, `.ff6` 12, `.ff7` 6, `.ff8` 6,
and `.ff9` 10. It also specifies arbitrary-size tiled `.ffx`, `.ffx2`, and `.ffxhd` images. The
commented original topic markup additionally identifies `.ffxi` and `.ffxii` as the level and
overworld toolbar backgrounds; these two dormant names are boundedly decoded but are not presented
as active original UI behavior.

## Command-to-cell evidence

The original uses zero-based cells. Only mappings with matching Rust actions are currently routed.

| Surface | Cell | Original command | Rust action | Evidence |
| --- | ---: | ---: | --- | --- |
| Overworld `.ff2` | 1 | `$23D2` | Save | `CreateOverworldEditorToolbar` at `$00560CF0` |
| Overworld `.ff2` | 2 | `$2464` | Undo | `CreateOverworldEditorToolbar` at `$00560CF0` |
| Overworld `.ff2` | 3 | `$2465` | Redo | `CreateOverworldEditorToolbar` at `$00560CF0` |
| Map16 `.ff5` | 2 | `$2261` | Save | 24-word command table at `$005E559C`; the dirty-state path enables `$2261` |
| Map16 `.ff5` | 3 | `$2279` | Undo | command table at `$005E559C` and Map16 history handlers |
| Map16 `.ff5` | 4 | `$227A` | Redo | command table at `$005E559C` and Map16 history handlers |
| Palette `.ff3`/`.ff6` | 2 | `$2279` | Undo | `FindPaletteEditorButtonIconIndex` at `$0056DE30`, table `$005E4134` |
| Palette `.ff3`/`.ff6` | 3 | `$227A` | Redo | `FindPaletteEditorButtonIconIndex` at `$0056DE30`, table `$005E4134` |
| Palette `.ff3`/`.ff6` | 11 | `$2261` | Save | `FindPaletteEditorButtonIconIndex` at `$0056DE30`, table `$005E4134` |
| Add Object `.ff7` | 1 | `$0067` | Show preview icons in list | `LoadCustomObjectToolbarIcons` at `$0052B7E0`, table `$005E5118` |
| Add Object `.ff7` | 2 | `$0069` | Compatible loaded GFX only | table `$005E5118`; object tooltip block `$005D1780` |
| Add Object `.ff7` | 3 | `$006A` | Use vertical layout | table `$005E5118`; object tooltip block `$005D1780` |
| Add Object `.ff7` | 4 | `$2440` | Preview zoom popup | table `$005E5118`; object-dialog handler |
| Add Object `.ff7` | 5 | `$0097` | Show preview area | table `$005E5118`; object tooltip block `$005D1780` |
| Add Sprite `.ff8` | 1 | `$0067` | Show preview icons in list | `LoadDialogCommandIconStrip` at `$00575B10`, table `$005E40A8` |
| Add Sprite `.ff8` | 2 | `$0069` | Compatible SP3/SP4 only | table `$005E40A8`; sprite tooltip `$005D8DC8` |
| Add Sprite `.ff8` | 3 | `$006A` | Use vertical layout | table `$005E40A8`; dialog handler |
| Add Sprite `.ff8` | 4 | `$2440` | Preview zoom popup | table `$005E40A8`; dialog handler |
| Add Sprite `.ff8` | 5 | `$0097` | Show preview area | table `$005E40A8`; dialog handler |

`LoadOverworldToolbarImages` at `$005608E0`, `LoadLevelToolbarButtonIconCaches` at `$004EA760`,
`LoadPaletteEditorButtonIcons` at `$0056DF40`, `LoadCustomObjectToolbarIcons` at `$0052B7E0`, and
`LoadLevelBackgroundToolbarIcons` at `$0051BCA0` independently corroborate the filenames and image
families. Rust leaves controls textual when an override is absent and deliberately does not assign
unverified cells to superficially similar actions.

## Rust verification

- `user_toolbar_images::tests::every_authenticated_editor_strip_and_tiled_gui_image_loads_at_its_exact_shape`
- `user_toolbar_images::tests::malformed_authenticated_editor_strip_rejects_without_publishing_any_set`
- `user_toolbar_images::tests::authenticated_editor_action_cells_match_the_decompiled_command_tables`
- `user_toolbar_images::tests::authenticated_catalog_action_cells_match_the_decompiled_command_tables`

The native portable overworld, Map16, and palette editors consume the authenticated cells for their
Save/Undo/Redo controls after texture initialization. The integrated standard, extended, and custom
object and sprite catalogs consume cells 1 and 3 for the matching preview-icon and vertical-layout
toggles; those settings affect the complete corresponding catalog family. Cells 2, 4, and 5 remain
unrouted until the GFX-compatibility predicate, catalog-preview zoom, and separate preview pane are
implemented. Nearest-neighbor sampling is retained for user-supplied pixel artwork, with text and
tooltip fallbacks.
