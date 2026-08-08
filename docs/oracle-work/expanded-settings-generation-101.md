# Expanded-settings generation 1.01 oracle

The compatibility evidence was produced with a historical Lunar Magic 2.22 distribution. No ROM
or executable is stored in this repository.

- Archive SHA-256: `9fc0a921f70fdaee1e9fbd871affbb414bb8b8f7dde49d96845b969e2d93e67b`
- `Lunar Magic.exe` SHA-256: `481ef77597ededdf681f15635357e10e17e2d3d770bc3b70d09f57ca800924ed`
- Default-placement headered ROM SHA-256: `8f0779c18a7e283a9f50211d8991d8faa9a86f2de2d65fcac1887e80f64d17ae`
- Forced-relocation headered ROM SHA-256: `19d448dc513644d63cafcdc8161032a7e845fad9224e82b86a95e06a5eba0699`
- Current-marker location: `$07F15C = FF FF FF FF`
- Companion generation marker: `$06FF37 = 4C 4D 01 01`
- Default allocation header/payload: `$0801D8/$0801E0`, length `$6D00`
- Relocated allocation header/payload: `$087FF8/$088000`, length `$6D00`

Procedure: open a headered pristine SMW-US ROM in Lunar Magic 2.22, enable Super GFX Bypass, and
save with one-megabyte expansion. For the relocation case, pre-expand a pristine logical image and
place an exact occupied `$6D00` RATS block at the default candidate before opening and saving. The
second result retains that block and places expanded headers at the next candidate. The opt-in Rust
test accepts both paths through `LM_EXPANDED_SETTINGS_101_ROMS`, authenticates their live operands
and complete immutable runtime family, migrates, semantically reopens current storage, injects a
late runtime corruption to prove rejection, and undoes to the exact physical input.
