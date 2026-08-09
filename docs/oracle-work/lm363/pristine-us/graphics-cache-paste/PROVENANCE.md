# Lunar Magic 3.63 diagnostic graphics-cache paste oracle

This retained observation was captured from the isolated Wine process previously used for the
current-level F9 oracle. It was not the user's Lunar Magic instance. That process held Lunar
Magic's own pristine-derived, 1 MiB expanded level-`$105` working ROM and an already-open
`Window8x8` editor. The ROM is unchanged by this observation because every operation below mutates
only Lunar Magic's decoded working graphics cache.

## Identities

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- expanded working ROM size: `0x100200`
- expanded working ROM SHA-256:
  `f8d7c8c85306115dbfcd41386ca73674e52f76760ccdebf0a6b66881ce51f288`
- `tools/wine-graphics-cache-oracle.c` SHA-256:
  `5e2d3dff3eb0a2fc711941a4ebf0b3d75f9b8dfae1d1919aadab136199ff5686`
- compiled 32-bit helper SHA-256:
  `13ad39a4f8bc0bcb666fcb761cdff2e5a1a9cca640b6db9740fb6d8f22567ad5`

The helper accepts an explicit Windows process ID or exact executable name, locates only that
process's `Window8x8`, and
records direct `WM_LBUTTONDOWN/UP` and `WM_RBUTTONDOWN/UP` transitions. It reads the active page,
feature flags, and planar graphics buffers with `ReadProcessMemory`; it changes only the transient
page selector while visiting pages and restores the original selector before exit.

## Observation

Command `$24E7` had already raised the maximum page from `$05` to `$3F`. The live feature globals
were Super GFX Bypass off, vanilla animation on, and Special World view off. A left click selected
tile `$000` as the edited source. Unmodified right-click then produced these exact outcomes:

- ordinary target `$002` changed and became byte-identical to the source;
- fixed vanilla-animation target `$041` remained unchanged;
- unused non-bypass FG/BG target `$300` remained unchanged;
- final current-level target `$5FF` changed and became byte-identical to the source;
- first out-of-range target `$600` remained unchanged.

The small `oracle.tsv` is the retained reviewable output. Ghidra independently binds the complete
predicate in `HandleGraphicsEditorWindowMessage` at `$005068C0`: right-paste rejects targets above
`$5FF`, `$300–$3FF` while Super GFX Bypass is off, `$41–$81`, `$90–$91`, `$DA–$DD`, and `$EA–$ED`
while vanilla animation is enabled, and `$480–$4FF` while Special World graphics are active.
