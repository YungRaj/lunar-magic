# Optimized-LZ2 runtime generation 1.00 oracle

This note records the non-redistributed historical evidence used by
`historical_lz2_speed_runtime_is_exactly_authenticated` and
`historical_lz2_speed_rom_authenticates_and_decodes_standard_graphics`. The ROM and complete IPS
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

This evidence therefore proves detection and standard-graphics readability, not reciprocal
migration parity. Rust's current ExGFX resolver cannot yet interpret this historical table
generation, and a codec migration must reproduce the coupled table/graphics-format upgrade before
the feature-parity row can be promoted.

## Verification

The exact synthetic fixture test authenticates the retained runtime and exercises corruption and
wrong-generation rejection. The opt-in corpus test authenticates the complete patch-derived ROM
and decodes every standard graphics stream through the selected optimized-LZ2 mode.
