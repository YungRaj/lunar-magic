# Custom sprite collection preview oracle

This fixture was captured from Lunar Magic 3.63 running under Wine against the authenticated,
headered pristine US Super Mario World ROM.

- Lunar Magic SHA-256: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- pristine ROM SHA-256: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- preview SHA-256: `e6a042be9e2192cc0d0b7413f299ab9857c33a96a6acf17da25d4da490fad3a6`
- preview framing: 520 by 520, 8-bit RGBA PNG

The Rust-authored sidecars contain one picker entry named
`Rust multi-sprite placement oracle` and one placement group with a green Koopa (`$00`) and a
Goomba (`$0F`) one 16-pixel cell apart. Its Rust-authored `.ssc` supplies two explicit display
tiles for each sprite, producing the retained blue-and-white four-tile preview instead of the
built-in artwork. `wine-custom-sprite-oracle.c` opens the original Add
Sprites window, activates category 4, selects that exact description, and publishes the native
`WindowSpriteViewx` rectangle. `capture-macos-window.swift` captures the exact Wine window through
ScreenCaptureKit and crops that rectangle at the compositor scale.

The same live transaction switches Lunar Magic into sprite editing mode with command `$2459`,
performs the picker-documented Ctrl+right-click at client coordinate `(96, 96)`, saves level `$105`,
and exports the before/after MWL files through Lunar Magic itself. The only sprite additions are
exactly `60 60 00` and `60 70 0F`; Layer 1, Layer 2, palette, exits, ExAnimation, expanded settings,
and the level header remain equal. The saved ROM retains a valid checksum.

Ghidra port 8089 independently binds the behavior to `LoadLevelCustomSpritePlacementMw2`
(`$005766B0`), `LoadLevelCustomSpriteDescriptionsMwt` (`$005767F0`),
`PopulateSpritePlacementDescriptionList` (`$00578EA0`), `ActivateSpritePlacementCategory`
(`$00579010`), `SpritePlacementDialogProc` (`$0057C830`), and `LevelEditorWindowProc`
(`$00498FA0`). The dialog tooltip explicitly requires Ctrl+right-click while sprite editing mode is
active.
