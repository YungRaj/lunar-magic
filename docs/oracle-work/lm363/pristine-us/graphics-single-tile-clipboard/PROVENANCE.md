# Lunar Magic 3.63 single-graphics-tile clipboard oracle

This observation was captured on 2026-08-08 from a fresh isolated Wine prefix. Lunar Magic opened
an unchanged copy of the authenticated vanilla US ROM and opened its original `Window8x8` through
command `$232A`; this was not the user's interactive Lunar Magic instance.

## Identities

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- vanilla ROM SHA-256:
  `7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7`
- `tools/wine-graphics-clipboard-oracle.c` SHA-256:
  `a618cddbbe669ba8242b86ff3a8215cce939da3722e975a04ce53d29ef116f36`
- compiled 32-bit helper SHA-256:
  `5aa0a8717a3c3e6a4a8046dd85c0d0d42ba0acefe453ad1ec2a16d6c35af0b99`

## Procedure and observation

The helper locates only that process's `Window8x8` and invokes the Ghidra-named original entry
points in the original process: `CopyEditedGraphicsTileToClipboard` at `$005051E0` and
`PasteEditedGraphicsTileFromClipboard` at `$005052B0`. It does not read or write Lunar Magic
process memory.

The initial copy published one registered `Lunar Magic 8x8 Tile` allocation of exactly 64 bytes;
the newly opened private edit tile contained 64 zero pixels. The probe then published four repeats
of the deliberately asymmetric indexed-pixel row `00 01 ... 0F`, invoked the original paste
function, invoked the original copy function again, and received all 64 bytes exactly. This proves
the original headerless pixel order and cross-process registered-clipboard boundary.

This is direct entry-point evidence, not a retained keyboard/menu gesture. Broader graphics-editor
interaction evidence therefore remains incomplete.
