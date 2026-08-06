# Lunar Magic editor-text cache

The first three fixtures are the dynamic `$3Cxx` Map16-sidecar cache materialized by Lunar Magic 3.63
under Wine after opening pristine SMW-US level `$104` with sprite previews enabled:

- `lm363-editor-text-definitions.bin`: 256 four-word definitions from process address `$00815E58`.
- `lm363-editor-text-glyphs.bin`: 79 8×8 indexed tiles from the sidecar `$880` page at `$006424B0`.
- `lm363-editor-text-palette.bin`: the 64-entry BGRA editor palette at `$0061F338`.
- `lm363-editor-font.bin`: 256 bold `System` glyph rasters and advances captured with Lunar
  Magic's `CreateFontA` settings (10 point, 96 DPI, `PROOF_QUALITY`) for viewport annotations.
- `lm363-overworld-sprite-map16-builtins.bin`: the exact 8,192-byte PE resource type `500`, ID
  `508`, loaded by `LoadBuiltInOverworldGraphicsResources @ $004BF9A0` as four built-in overworld
  Sprite Map16 pages (`$000..$3FF`).

Ghidra evidence comes from `RenderM16SidecarObjectsToPixelBuffer @ $0044F5A0`, which resolves
definition words through the `$880` sidecar page and the dynamic editor palette. These are Windows
editor assets; treating `$3Cxx` as ordinary SNES sprite tile words produces corrupt previews.

SHA-256:

- definitions: `16f38904ba0befbb7423b510c96bc9ffaaedec686651a6d67fe943f741894e62`
- glyphs: `737964b2149ba1e2a7b9482783acf93af6eac501a03ff72e3ae7f5124bf838c9`
- palette: `ffb86b97d436d7d1333d040c12126a2316a4d1a702d26047655cfe8f7112e0a2`
- font: `442f07d7ccb54c97f1237c2b3ad879593b1e714460824d4e156e1d856ac4e52d`
- overworld Sprite Map16 built-ins: `d23b64559ac8a95d2011842cd4731f29914a45ac94cc74e7beff80ed54037d4b`
