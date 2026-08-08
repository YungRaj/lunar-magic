# Lunar Magic 3.63 graphics pixel-edit buffer oracle

This retained observation was captured from the isolated Wine process used for the current-level
F9 oracle, not from the user's Lunar Magic instance. The process held Lunar Magic's own
pristine-derived, 1 MiB expanded level-`$105` working ROM and an unlocked `Window8x8` editor.
The helper selected diagnostic tile `$600`, sent the original `x` transform twice, painted its
upper-left pixel with foreground color 1 and restored it with background color 0.

## Identities

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- expanded working ROM size: `0x100200`
- expanded working ROM SHA-256:
  `f8d7c8c85306115dbfcd41386ca73674e52f76760ccdebf0a6b66881ce51f288`
- `tools/wine-graphics-pixel-oracle.c` SHA-256:
  `33605e76e9dac1e70b8be6516d180ae6b43ec8040c0599434e6032fdfd4fa994`
- compiled 32-bit helper SHA-256:
  `0d6366217b708bce5269324527aca4fd012537fc7e1750283334c371a81e6320`

The helper accepts an explicit Windows process ID and locates only that process's `Window8x8`.
It uses `ReadProcessMemory` to compare Lunar Magic's 64-byte selected-tile edit buffer at
`$00ACF908` with the decoded and planar backing caches for tile `$600`. It restores the original
page selector before exiting and does not commit or alter the working ROM.

## Observation

The first horizontal flip changed the private edit buffer while both backing representations
remained unchanged; the second flip restored the buffer. Foreground painting changed pixel zero
in the edit buffer from 0 to 1 while decoded and planar backing remained unchanged. Background
painting restored the edit buffer. This establishes that selection, transforms, and pixel painting
stage one private edited tile. Backing graphics change only through Lunar Magic's separately
guarded right-paste operation captured by `graphics-cache-paste/oracle.tsv`.
