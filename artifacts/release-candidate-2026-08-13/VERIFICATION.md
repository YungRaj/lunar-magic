# Whole New World release-candidate verification

- Source ROM: `sysLMRestore/smwOrig.smc` (authenticated vanilla SMW-US)
- Source SHA-256: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- Rebuild recipe: `build-whole-world.commands`
- Result ROM: `whole-new-world.smc`
- Result SHA-256: `fab9435587514537ee0151c9ea9f03748346ecc01339fefc9c3a8b63fe80f5e5`
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
- Retained Libretro oracle entered sublevel 105 at translevel 28, switched to 106, returned to 105 deterministically, and produced final level frame SHA-256 `8d279d01ce16594cbfd6409f5a2b2f96d7aa17c92a83acdee5de2aba5bf25f67`.
- The opening Koopa was moved from X `$0D` to unobstructed ground at X `$09`; a 180-frame runtime probe produced changing frame hashes while its active sprite slot remained healthy, and the post-fix full-level audit again produced 20 distinct screen hashes.
- The integrated Rust `Live ROM Test` visibly entered sublevel 105 and rendered the remodeled terrain, palette, and Koopa.
- The final screencast was decoded and sampled at eight timeline positions after assembly. It contains no original Lunar Magic or Wine footage.
