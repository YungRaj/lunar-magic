# Map16 stage-3 to stage-4 compatibility oracle

This retained fixture makes the Lunar Magic 3.01 `$0111` to Lunar Magic 3.63 `$0112` Map16 compatibility transition self-contained and mandatory.

## Executable authority

- Official Lunar Magic 3.01 archive: `https://web.archive.org/web/20190117220628id_/https://fusoya.eludevisibility.org/lm/download/lm301.zip`
- Archive SHA-256: `2eacccec3d8770667bf496e14b8983586376dac7d5303528bc116571d91a4b8f`
- Lunar Magic 3.01 executable SHA-256: `eb036287726ba87187aa875f37ab0bebdb21748c19e73503ca4c2177fcc927af`
- Lunar Magic 3.63 is the repository's established original-editor oracle.

## Stage-3 construction

Lunar Magic 3.01 opened the canonical-header pristine SMW-US revision-0 ROM with SHA-256 `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`. F9 was sent to the modeless `Window16x16`, the `Save Map16 data to ROM?` prompt was accepted, and the required expansion to 1 MiB was accepted. The resulting ROM:

- has physical size `0x100200`;
- has SHA-256 `2e5f007de816d14d7804ec95daee756247996396eafe7ab102cafeb38cd45c68`;
- authenticates the complete stage-3 hook network;
- contains marker `4C4D1101` at logical `$03765C`.

## Stage-4 upgrade

Lunar Magic 3.63 first exported that ROM's unchanged complete Map16 set with `-ExportAllMap16`; the 651,760-byte file has SHA-256 `dc625b0dcbbc31ffa12ff817e824302f5fd6dbecc274af3f2489378a6e17f026`. A copy of the stage-3 ROM then imported that exact file with `-ImportAllMap16`.

The upgraded ROM has SHA-256 `7c630019c6fae2feefd61193ba5d72b23d9e13e0a16bf4f9672056d9452b19c5`. Its complete 62-byte before-to-after IPS has SHA-256 `1bd84dd0afe4bd55022e86fed20bff1466b697e5048d79bd1f84b8708079bac2` and is retained at `crates/lm-profile/src/assets/map16_stage3_to_stage4.ips.b64`.

The six changed physical ranges are `$03785E..$03785E`, `$0379A3..$0379A4`, `$0379A6..$0379B3`, `$07F1F8..$07F1FE`, `$07F2B6..$07F2B7`, and `$07F2C3..$07F2C4`. The first three are exactly the logical `$03765E` version byte and `$0377A1..$0377B3` stage-4 helper tail; the final three are Lunar Magic editor-version metadata. No Map16 definitions, Acts Like data, allocations, or other runtime bytes changed.
