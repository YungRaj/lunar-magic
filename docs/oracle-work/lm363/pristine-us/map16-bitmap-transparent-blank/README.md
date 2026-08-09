# Lunar Magic 3.63 transparent bitmap blank-reuse oracle

Source program: authenticated `Lunar Magic.exe` 3.63 in the labeled Ghidra project served at
`127.0.0.1:8089`. This fixture records static original-program evidence and does not claim a live
Wine interaction capture.

The recovered decision chain is independent across the two blank-reuse controls:

- `DeduplicateImportedGraphicsTiles` at `004EE470` checks `DAT_005E55F7`. For an all-zero decoded
  8×8 tile, the enabled branch writes `DAT_005E55EC | 0x10000000` into the graphics mapping table.
  `AllocateImportedGraphicsTileSlots` at `004EEE40` recognizes that marker and emits the configured
  tile index without allocating or writing a graphics slot. With the switch disabled, the ordinary
  free-slot branch allocates the zero tile.
- `AllocateImportedGraphicsTileSlots` recomputes the occupied byte at `DAT_009B8588[index]` from
  the encoded planar bytes, so either form of transparent-source graphics remains classified blank.
- `ImportBitmapAsDeduplicatedMap16Tiles` at `004EF2D0` independently checks `DAT_005E55F8` and all
  four `DAT_009B8588[subtile & 0x3FF]` bytes. When enabled and all four are zero, it writes
  `DAT_005E55F0` directly into the output layout and does not call `FindNextBlankMap16Tile` or write
  a definition. When disabled, the ordinary unique-block path allocates a blank definition.
- `RunBitmapToMap16ImportWorkflow` at `004F47B0` calls graphics allocation before the deduplicated
  Map16 importer, establishing the ordering assumed by the truth table.

Consequently each checkbox controls only its own allocation domain, producing the four rows in
`oracle.tsv`. The Rust regression binds those rows to semantic commit/reopen and exact Undo across
headered and headerless vanilla ROMs.
