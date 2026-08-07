# Lunar Magic 3.63 custom-palette legacy level fixture

These five files were exported live from the 32-bit `Lunar Magic.exe` 3.63 under Wine. The source
was `palette-install-positive/after.smc`, the authenticated pristine-US-derived ROM in which Lunar
Magic installed level 000's custom palette.

The capture used an isolated copy of the executable named `LMCustomPaletteOracle.exe`. After
opening level `000`, the legacy-export mode byte at process address `0x00e278d7` was set to `1`,
command `0x2395` (`Save Level to File`) was dispatched, and the native Save dialog published
`Level 000.mwl` plus its four sidecars. The process was then closed without saving the ROM.

- `Level 000.mwl`: text manifest
- `Level 000.mw0`: Layer 1 payload
- `Level 000.mw1`: Layer 2 payload
- `Level 000.mw2`: sprite payload
- `Level 000.mw3`: exact 257-word custom-palette payload

This capture also demonstrates that current legacy manifests accept the complete 13-bit
secondary-exit namespace through `$1FFF`. The source ROM's installed table caused Lunar Magic to
emit 7,923 exit rows, including zero-valued records; those rows are retained exactly as oracle
evidence.

SHA-256:

```text
38a203f968425a74bce6345426419a5f01e66eb9c5808423f64ab04e088199a8  Level 000.mw0
67cb940c874127ebca7fdaf0da44e1f683c5040fd36cb746b5222ffd055cfffe  Level 000.mw1
640d08e6bc267e92d441e755bbefca1288d3fc3f45f0ef080bd93ddd6e532faf  Level 000.mw2
43c981cb1409b77459907a2d18d401796eee85f3fe29de030123a10d9afa7a07  Level 000.mw3
5250f9c35ffa3254753599f6186cf1fa04255f6d17c06760a0b0c3a5793c18d2  Level 000.mwl
```
