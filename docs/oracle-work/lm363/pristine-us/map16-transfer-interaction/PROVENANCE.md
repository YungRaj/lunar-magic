# Lunar Magic 3.63 Map16 transfer interaction oracle

This observation was captured on 2026-08-08 in a disposable Wine prefix. Lunar Magic opened an
isolated copy of the authenticated expanded vanilla US ROM and command `$232F` opened its real
modeless `16x16 Tile Map Editor`. No user ROM or interactive Lunar Magic process was used.

## Identities

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- initial ROM SHA-256:
  `7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7`
- required headered restore ROM SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- `tools/wine-map16-transfer-oracle.c` SHA-256:
  `98d2a739d4699eea553968b2477f2f8268d339a5037b40c4988d4defdef2a407`
- compiled 64-bit helper SHA-256:
  `fcbc790a2bc59788f9c06dca76e7ef0948edc742f566cffce61950c7cd6a24ba`

## Original interaction boundary

The helper locates only the named Lunar Magic process, the visible Map16 `#32770` dialog, its
`Window16x16view`, the native Select Tiles popup, and the standard Open/Save As dialogs. It uses
ordinary button, combo, edit, mouse, and file-dialog messages. It does not read or write Lunar
Magic process memory. The observed buttons dispatch `$2266/$2267` for current-selection
export/import and `$2268/$2269` for complete export/import. Standard dialogs use modern filter
index 0 and legacy raw filter index 1.

The modern selected route exported tile `$0200` as a 1x1, 186-byte LM16 file. Its ten semantic
bytes are `04100410041004103001`; selecting `$0200`, importing that file, and exporting again was
byte-exact. The complete route exported a 651,760-byte file. Importing it through the blue button
and exporting again was also byte-exact. Complete import immediately saved the ROM, changing its
SHA-256 from the retained before digest to the retained after digest.

The missing-restore boundary was captured separately before `sysLMRestore/smwOrig.smc` was placed
beside the isolated ROM. Complete import stopped at the exact `Restore System Issue` prompt in the
TSV. Cancel returned to the editor without applying the import. With the authenticated headered
restore ROM present, the same import completed without another prompt.

For the legacy route, the exact 16x16 page selection produced `Map16Page.bin` (2,048 definition
bytes) and the automatically named `Map16PageG.bin` (512 Acts-Like bytes). The regular import
accepted that pair. Select Tiles menu item 0 (`Select all FG tiles`) produced `Map16FG.bin`
(262,144 bytes) and `Map16FGG.bin` (65,536 bytes); importing the pair and exporting the same FG
selection reproduced both files byte-for-byte. Menu item 1 (`Select all BG tiles`) did the same for
`Map16BG.bin` and the optional/discardable `Map16BGG.bin`, including byte-exact re-export. The menu's
third item was visibly `Select all tiles`; the retained complete modern route uses the dedicated
blue buttons because those include tileset-specific editor state and save on import.

The TSV stores exact sizes and SHA-256 identities rather than committing ROM-derived binary data.
The source helper provides a reproducible control path; the Rust fixture test binds these observed
boundaries to the implemented transfer models and GUI actions.
