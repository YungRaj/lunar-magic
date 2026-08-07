# Sprite 19 shared-runtime command `$26AC` oracle

This fixture records Lunar Magic 3.63's live `PromptAndInstallSprite19AsmFix` transaction when its shared helper is already authenticated. It closes the second required source-state permutation for the Sprite 19 ASM-fix gate.

## Source construction

- Base: canonical-header SMW-US revision 0, SHA-256 `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`.
- Logical `$00E762` was changed from `8D111F8DB81F` to `EA22A0BC03EA`.
- Logical `$01BCA0..$01BCBF` was changed from 32 `FF` bytes to `AD0901F010AFF09E008D111F8DB81F6B22C99B00FA6BFFFFFFFFFFFF4C4D1101`.
- Logical `$0020A0` remained pristine at `9C111F`.
- The Rust `checksum-auto` command repaired the checksum before Lunar Magic opened the ROM.
- Checksum-valid before SHA-256: `4d7f6277c064aa3276e8c4d72e6201d6d95b784e83de97cbd302565c4812bd15`.

## Original transaction

The ROM was opened in an independently named Lunar Magic 3.63 process under Wine. The expected modified-ROM warning was acknowledged, hidden command `$26AC` was posted to the `LMFrame`, and the `Install Old Fix for Sprite 19?` prompt was accepted.

Lunar Magic wrote the file immediately. After SHA-256: `69c71c25b313221d1508a26a2fde9534b7ed811fa1ddb44f521c041d070339fe`. The complete deterministic before-to-after IPS is retained as hexadecimal at `crates/lm-profile/src/fixtures/sprite19_shared_command_26ac.ips.hex`.

The changed physical ranges were:

- `$0022A0..$0022A2` (logical `$0020A0..$0020A2`): `9C111F` to `EAEAEA`.
- `$07F222..$07F33F`: Lunar Magic 3.63 attribution/version footer created on first modification.
- `$0801E7..$0801E9`, `$0801EB..$0801F1`, and `$0801F3..$0801FF`: editor metadata and checksum fields created on first modification.

Neither the authenticated hook nor the shared `$20`-byte helper changed. Therefore the fix-specific original behavior for this source state is exactly the one-write Rust plan at logical `$0020A0`.
