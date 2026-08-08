# Lunar Magic 3.63 Map16 editor interaction oracle

This observation was captured on 2026-08-08 from a fresh isolated Wine prefix. Lunar Magic opened
an unchanged copy of the authenticated vanilla US ROM and opened its modeless `16x16 Tile Map
Editor` through command `$232F`; this was not the user's interactive Lunar Magic instance.

## Identities

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- vanilla ROM SHA-256:
  `7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7`
- `tools/wine-map16-editor-oracle.c` SHA-256:
  `7cfc6ff204acfea03dddd1e106b370657e56c1f086f844adc8acde83d59f064e`
- compiled 32-bit helper SHA-256:
  `97b9d83eed7bbb5520aac08f3fb862c420bcb858bf248a4a203b110b7964a53e`

## Procedure and observation

The helper locates only that process's visible `#32770` Map16 dialog and its
`Window16x16view` child. It interacts through ordinary control and mouse messages and reads only
the visible control state; it neither reads nor writes Lunar Magic process memory.

The exact page control and drag-selection path selected page `$02`, tile `$200`. The selected tile
began with subtiles `$192,$193,$194,$195`, Acts Like `$1F0`, palette selection 3, and priority
selection 1. The helper then made nine distinct GUI edits: four replacement subtile indexes
`$123,$234,$345,$056`, Acts Like `$130`, palette selection 4, priority selection 2, horizontal
flip, and vertical flip. The flips produced the visible quadrant order `$056,$345,$234,$123`.

The original enabled its Undo control for exactly nine steps. Repeated Undo restored every visible
field and combo selection exactly to the initial state and disabled Undo. The resulting Redo stack
contained exactly nine steps; replaying it restored every modified value exactly and disabled Redo.
Together with the named Ghidra control handlers and the separate exact clipboard oracle, this
retained run covers page/tile selection, subtiles, palette, priority, flips, Acts Like, Undo, and
Redo across the complete core Map16 editing workflow.
