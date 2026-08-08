# Expanded-settings generation 1.02 oracle

The retained compatibility evidence was produced with the official Lunar Magic 2.42 archive. No
ROM or executable is stored in this repository.

- Archive SHA-256: `8eb31b2a6010caed13f304a18361378e5a542acf47d9831355fdd9a467495ee0`
- `Lunar Magic.exe` SHA-256: `7a3444468038e11eb34592906ed95d062abde4cb04fc6bcdfe4c6432c0e780cf`
- Generated headered ROM SHA-256: `68336177bbdee516442a1d50f0fb1be7dfead153347f912f18e7ade21e3087d8`
- Logical generation marker: `$07F15C = 4C 4D 02 01`
- Logical allocation header/payload: `$0801D8/$0801E0`
- Allocation payload length: `$6E00`

Procedure: open a headered pristine SMW-US ROM in Lunar Magic 2.42, save once to expand it, reopen
the result, enable Layer 3 GFX/Tilemap Bypass through command `$2533`, and save. Static disassembly
of the same executable independently shows installer constant `$01024D4C`. The opt-in Rust test uses
`LM_EXPANDED_SETTINGS_102_ROM` to run strict detection, atomic migration, current semantic reopen,
and byte-exact undo against this external fixture.
