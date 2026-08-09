# Optimized-LZ2 runtime generation 1.00 oracle

This note records the non-redistributed historical evidence used by
`historical_lz2_speed_runtime_is_exactly_authenticated` and
`historical_lz2_speed_rom_migrates_all_graphics_and_events_like_lunar_magic`. The ROM and complete IPS
patch remain outside the repository. The retained 431-byte runtime is the minimum exact byte
fixture needed to authenticate this runtime generation and its rejection boundaries.

## Source

The public `AVSMWFinal.ips` patch was obtained from the SMW Central Mirann hack archive:

`https://media.smwcentral.net/Mirann/Hacks/AVSMWFinal.ips`

It was applied with `lm-cli ips-apply` to the authorized local, copier-headered SMW-US revision-0
ROM.

| Artifact | SHA-256 |
| --- | --- |
| Base headered ROM | `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf` |
| Public IPS patch | `6bbfd73b08a389c0973df990effc8e79e85686b4f78901868775c36571d7f5d4` |
| Patch-applied ROM | `429fbbaf252cc6c9bed24c220d60f6b84626f9a15efc8aa5ac85295ccb3844e8` |

## Authenticated runtime

The 2 MiB LoROM carries graphics-compression metadata value 1 and the five-byte hook
`22 08 80 90 60` at logical `$0038E3`. The JSL target is logical `$080008`, immediately after a
valid RATS header at `$080000`. Its exact owned payload is `$080008..$0801B7`:

| Property | Value |
| --- | --- |
| Payload length | `$1AF` (431 bytes) |
| Payload CRC-32 | `b5f7eda1` |
| Payload SHA-256 | `7aaeae2444099f92a3f08406a92729cfaf5072e1988c9acc3dced1408ca5ee02` |
| Trailer | `4c 4d 00 01` (`LM 00 01`) |

Rust requires the metadata, hook target, exact RATS ownership, exact length, generation trailer,
and complete payload CRC to agree. A changed runtime byte rejects as a checksum mismatch, while a
current-generation trailer on this legacy-length payload rejects as a generation mismatch. The
authentic ROM opens with the optimized-LZ2 decoder and all 50 ordinary graphics streams plus
GFX33/GFX32 decode without mutating the source.

## Lunar Magic 3.63 conversion boundary

Original Lunar Magic 3.63 accepted this ROM and converted it with:

```text
Lunar Magic.exe -ChangeCompression legacy.smc LC_LZ3
```

The resulting ROM SHA-256 is
`459afe624144db588da712550312728a325d8ecdd1170221ff681ecaa188c488`, reports compression mode 2,
and installs hook `22 53 f7 a9 60`. Original-editor exports before and after conversion contained
52 standard GFX files and 54 ExGFX files. Every ExGFX file was byte-identical and 51 of 52 standard
GFX files were byte-identical. GFX17 changed from
`953a014c7cb8e9c28ec59d08dfded345f88cdcd1e00feb843287861845a7fb24` to
`559d1f7ee9e6875b27c89563359d21d7adc3275a2b3fb97a35b320188d85a30`; the changed fourth-plane
bytes show that the original conversion couples the codec transition to a legacy graphics-format
upgrade.

The extended files reveal the legacy table layout. Thirty-three files are in the ordinary
`$80..$FF` domain; 21 more are in `$100..$132`. The live `$07F873` operand selects a relocated
`$6D00` RATS settings owner whose first `$2D00` bytes hold the complete extended pointer table.
ExGFX120 and ExGFX127 decode to `$FFF` bytes; the other 52 files decode to `$1000`. Lunar Magic
preserves all 54 byte-for-byte rather than rejecting or padding the two older shapes.

The overworld event tilemaps use another retained generation. Relative to the current loader, its
64-byte primary runtime changes bytes `$1D/$3C` to `$80`, its index/reveal/state JSL banks use
`$85/$83/$83`, and reveal-runtime byte `$16` is zero. Those bytes are unchanged by the original
LZ3 conversion, while both compressed event streams move and reopen under LZ3.

Rust now follows both authenticated settings-owner lengths (`$6D00` legacy and `$6E00` current),
accepts either exact event-loader generation, preserves bounded pre-existing ExGFX streams up to
`$1000`, and applies the four-tile GFX17 upgrade. Its same-size forward migration matches all 52
standard files, all 54 ExGFX files, and both event buffers from the Lunar Magic oracle; checksum,
semantic reopen, and exact Undo pass. The Rust result SHA-256 is
`67a7c8bd72e4902b3dc28165f952ab0d063ddfd5b54e9656e50a24f2df843563`. Lunar Magic 3.63 reports
that file is already LZ3, leaves the hash unchanged, and re-exports all 106 graphics files exactly.
Rust also reverses that LZ3 result to both current LZ2 modes while preserving the upgraded GFX17,
all 54 legacy ExGFX streams, and both event buffers. Lunar Magic's independent reverse-oracle
SHA-256 values are `b32e39a355bce5f4c9de766076e35b25e605feeb9ffdaa7cd9dc391291db3baa`
for `LC_LZ2_Orig` and `718926f479da582d52245ba441e3821b57f0d4de8b60f2e4cb298108be424206`
for `LC_LZ2_Speed`. Rust's semantically equivalent results are
`391c4bac9719894f8c63b5c4fc56ea59b576477d55a0eb0a9ffd137973fbd408` and
`58bfd1818513fde7936a4d852c44bf663da9236be2679a0c55684c18d52138f1`; Lunar Magic recognizes
both as already using the target format, leaves both hashes unchanged, and re-exports 52/54 files
that byte-match the corresponding original-editor reverse oracle. Other unobserved historical
runtime generations remain outside this completed generation boundary.

## Verification

The exact synthetic fixture tests authenticate the retained compression and event runtimes, both
settings-owner lengths, the GFX17 four-tile transformation, corruption rejection, and
wrong-generation rejection. The opt-in corpus test authenticates the complete patch-derived ROM,
migrates it to LZ3, compares every graphics/event domain with Lunar Magic 3.63, and undoes to the
byte-exact source. It then migrates that exact LZ3 result to both LZ2 modes, compares the same
domains against the two retained reverse oracles, and independently undoes each to the byte-exact
LZ3 input.
