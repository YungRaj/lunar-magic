# Lunar Magic 3.63 single-Map16-tile clipboard oracle

This observation was captured on 2026-08-08 from an isolated Wine prefix. Lunar Magic opened an
unchanged copy of the authenticated vanilla US ROM; this was not the user's interactive Lunar Magic
instance.

## Identities

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- vanilla ROM SHA-256:
  `7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7`
- `tools/wine-map16-clipboard-oracle.c` SHA-256:
  `ef018d57c85404e37a8fbaa60a76574b41028bd6944b593c1b01126a314a0041`
- compiled 32-bit helper SHA-256:
  `fffda3f791e43c5f9cbafff3ebfa35947814be8ad5e58902eaf071c5867967ca`

## Procedure and observation

The helper locates only that process's `Window16x16`, selects its first tile through ordinary mouse
messages, and invokes the Ghidra-named original entry points in the original process:
`CopySelectedMap16TileToClipboard` at `$004E6DD0` and
`PasteSelectedMap16TileFromClipboard` at `$004E6EB0`. It does not read or write Lunar Magic process
memory.

The first copy published one registered `Lunar Magic 16x16 Tile` allocation of exactly ten bytes:
`70 1C 72 1C 71 1C 73 1C 00 00`. The probe then published the deliberately asymmetric record
`23 01 67 45 AB 89 EF CD 57 13`, invoked the original paste function, invoked the original copy
function again, and received that record byte-for-byte in another exact ten-byte allocation. This
proves the original copy/paste record order and cross-process registered-clipboard boundary.

This is direct entry-point evidence, not a retained keyboard/menu gesture. Broader Map16 interaction
evidence therefore remains incomplete.
