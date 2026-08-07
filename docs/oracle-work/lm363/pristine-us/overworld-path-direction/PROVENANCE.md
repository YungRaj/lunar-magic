# Lunar Magic 3.63 overworld path-direction oracle

- Executable: `Lunar Magic.exe`, 3,162,112 bytes, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Source logical ROM: pristine SMW US revision 0, SHA-256
  `0838e531fe22c077528febe14cb3ff7c492f1f5fa8de354192bdff7137c27f5b`.
- Lunar Magic-added-header source: 524,800 bytes, SHA-256
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`.
- Saved result: 1,049,088 bytes, SHA-256
  `7503b8514d8c1221543babcdc01c0ec8f92e8d9db450b7bccbc40e8fb69f12ab`.
- Capture date: 2026-08-07.

The run used an isolated Wine prefix. After opening the authentic Overworld Editor, selection mode
2 was established and Layer 1 record `$009D` (tile type `$83`) was selected through the editor's
stride-two cell-state table. Command `$2074` opened `Submap Exit Tile Settings` after the original
`ValidateSelectedOverworldWarpTile` predicate accepted the tile.

The source-link combo retained index 5 (native link 4). Direction control `$0068` changed from
index 2 (`Left`) to index 0 (`Up`), and return-endpoint control `$0069` changed from index 6 to
index 0 (one-way). Pressing OK followed by overworld Save command `$2392` expanded and saved the
ROM. Rust's native path exporter decoded both ROMs; only link 4's source and destination changed.
The target bytes and all thirteen other interleaved path records remained exact. The semantic
X/Y fields in `transition.tsv` account for SMW's native Y/X endpoint planes and Y/X target-byte
order; `interleaved_record_hex` retains the original raw order. ROM images are not retained.

For the rejection branch, Layer 1 record `$0000` (tile type `$00`) was selected and command `$2074`
was invoked again. Lunar Magic displayed title `Wrong type of tile!` with body
`An exit tile must be selected to use this.` The complete ROM SHA-256 was identical before and
after dismissal, proving that rejection did not mutate the file. Ghidra's live 256-byte lookup at
`DAT_b53270`, indexed by the low byte of the stride-two Layer 1 cell table in
`ValidateSelectedOverworldWarpTile` (`$00538B00`), accepts exactly tile types `$25`, `$40`, `$42`,
`$43`, `$44`, `$45`, `$46`, `$47`, `$48`, `$4D`, `$52`, `$53`, and `$83`.
