# Lunar Magic 3.63 overworld ExAnimation runtime oracle

This observation was captured on 2026-08-09 from the authenticated 32-bit original editor under
Wine, using a copier-headered pristine SMW-US revision-0 ROM. The proprietary ROMs are not
retained in the repository; the exact owned-byte fixtures required by the automated comparison
are retained as base64 source assets.

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Copier-headered pristine input SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`.
- Ordinary Lunar Magic-expanded baseline SHA-256:
  `7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7`.
- Saved edited output SHA-256:
  `93e5daddf0229d34232e83f4e40c6d3d7321807dd92644981fe9d1211eb20d5b`.

The retained Win32 helper `tools/wine-overworld-animation-runtime-oracle.c` opened level `$105`,
opened the Overworld Editor, then opened `Edit Submap Extended Animation Frames (in hex)`. It set
Type index 1, Destination `00A0`, Frames `00`, and source frame `0500`, accepted the dialog, issued
the overworld Save command, and accepted `Save overworld to ROM?`.

After subtracting the copier header, Lunar Magic installed three core RATS allocations:

- runtime header `$08BC66`, payload `$08BC6E`, length `$C20`;
- auxiliary header `$08C88E`, payload `$08C896`, length `$15`;
- options header `$08C8AB`, payload `$08C8B3`, length `$07`.

Their complete contiguous captured bytes have SHA-256
`04fb09d57cb18d8d6f6a07cc00c5f15767075a8764182cfb329c8253eb342b26`. The runtime payload alone
has SHA-256 `9d84a3d1104279fe1c578714cbca0ebf06c549bea992486bcaa06f3e2efd5501`.
The exact fixed writes are JSL target bytes `22 6E BC 11` at `$020086`, `22 5E BE 11` at
`$0024E3`, operand `6E C1 11` following the JSL opcode at `$0200E0`, and mode bytes `$14` at
`$020102`, `$02010D`, and `$02013B`.

The differential exposed that the `$15` auxiliary payload is not immutable padding. It is seven
mutable 24-bit submap pointers. An unused entry has the exact sentinel `FF 00 00`; the edited first
entry is `C2 C8 11`, which resolves to logical payload `$08C8C2`. A fourth adjacent RATS block at
header `$08C8BA` owns that `$11`-byte compact ExAnimation payload. Its complete header and payload
have SHA-256 `e6d3ad990be851cbb03cb9d1656eb05bfd0fa16dda71da82163ed3dfc50b980b`.

`authentic_lunar_magic_363_runtime_matches_every_owned_and_fixed_byte` normalizes only that one
deliberately mutable pointer to compare Rust's pristine installer against every captured core-owner
byte, then reapplies the exact pointer and compact owner and requires the Rust detector to expose
the resulting ownership chain. Malformed empty sentinels and non-RATS pointer targets fail closed.
