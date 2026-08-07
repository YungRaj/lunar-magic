# Lunar Magic 3.63 overworld warp-link oracle

- Executable: `Lunar Magic.exe`, 3,162,112 bytes, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Source: Lunar Magic-added-header pristine SMW US revision 0, 524,800 bytes, SHA-256
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`.
- Saved result: 1,049,088 bytes, SHA-256
  `6dcc0b187d1a5fd3d65aff7e5343728388bc0cef1b7c24c615463ec41d46c293`.
- Wine: staging 11.13, isolated 64-bit prefix with the GDI renderer.
- Capture date: 2026-08-07.

The original Overworld Editor was switched to Layer 1 selection mode. Its selection state was
established through the same stride-two table consumed by
`RequireSingleSelectedOverworldLevelTile`: record `$003A` (raw Layer 1 type `$82`) was submitted
through command `$2076`, followed by record `$00EB` (also type `$82`). The first command entered
two-click mode 2; the second opened the original `Link Star and Pipe Tiles` dialog.

The dialog initially selected combo rows 3 and 5. They were changed to rows 27 and 28, which map
exactly to native records `$19` and `$1A`. Pressing OK cleared the previous owners in records `$01`
and `$03`, installed the selected tile pair in `$19/$1A`, and left every other record unchanged.
Overworld Save command `$2392` published the result. Rust exported the original and result through
the detected four-plane loader into canonical `LMOWWR1` values; `transition.tsv` retains both full
files as compact hexadecimal evidence instead of retaining either proprietary ROM.

For rejection, Layer 1 record `$0000` (type `$00`) was selected and command `$2076` invoked. Lunar
Magic displayed `Wrong type of tile!` with body
`A star, pipe, or exit tile must be selected to use this.` Dismissing it and issuing `$2392` left
the complete saved ROM SHA-256 unchanged. The transition and rejection together cover the original
success and failure boundaries without relying on an inferred write.
