# Whole New World release-candidate verification

- Source ROM: `sysLMRestore/smwOrig.smc` (authenticated vanilla SMW-US)
- Source SHA-256: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- Rebuild recipe: `build-whole-world.commands`
- Result ROM: `whole-new-world.smc`
- Result SHA-256: `c676732efa081f6c9ca306918529d94f9be66a2f0599774d8db4dfe79f4e9c89`
- Screencast: `our-rust-lunar-magic-whole-new-world-final-verified.mov`
- Screencast SHA-256: `4217f4fb5a17f6c2e285ccbfca85086258ba77faf2d683df7a2a0b3728bff5a4`
- Screencast duration: 1530 seconds (25:30)

## Passed gates

- The build starts from the authenticated vanilla ROM and applies edits with the Rust tooling only.
- Playable entrance aliases `$104` and `$105` both contain the remodeled level.
- Level 105 spans game screens `$00` through `$13`, with 76 Layer 1 objects and 37 sprites.
- Terrain, slopes, platforms, pipes, blocks, obstacles, enemies, goal structure, time, music, background/foreground/sprite palette selectors, and background color are modified.
- The playable main overworld Layer 2 data contains the edited five-stage Yoshi's Island route and terrain.
- Save/reopen full-level audit produced 20 images and 20 distinct image hashes; see `final-audit/manifest.tsv` and `final-audit/contact-sheet.png`.
- Retained Libretro oracle entered sublevel 105 at translevel 28, switched to 106, returned to 105 deterministically, and produced final level frame SHA-256 `9c7363733297ba6fb1de01a75d6ffc69e4d768894e78e277f7351467ea24f6a8`.
- The integrated Rust `Live ROM Test` visibly entered sublevel 105 and rendered the remodeled terrain, palette, and Koopa.
- The final screencast was decoded and sampled at eight timeline positions after assembly. It contains no original Lunar Magic or Wine footage.

