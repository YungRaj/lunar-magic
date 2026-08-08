# Expanded-settings generation 1.00 oracle

This note records the non-redistributed historical evidence used by
`smw_us_v1_expanded_settings_generation_100_migration`. ROM images and patches remain outside the
repository.

## Sources

Two independently authored hacks whose final editor is identified as Lunar Magic 1.71 were used:

- Castle of WTF (Rev 1):
  `https://smwdb.me/db/4/4b24423f46700231f97f2d63cc90852fe686b4b7/`
- Karoshi Mario:
  `https://smwdb.me/db/8/8fa669d86edce50e6d61ba2855817ffe59dc2e30/`

Only their IPS patches were downloaded, through the Internet Archive's preserved `patch.ips`
objects, and applied to the authorized local headered SMW-US revision-0 fixture with `lm-cli
ips-apply`.

| Oracle | IPS SHA-256 | Patched ROM SHA-1 | Patched ROM SHA-256 |
| --- | --- | --- | --- |
| Castle of WTF | `fd36d8cf604def1708b0aef333176999339e5d28eb4b99dca4107e2b37f48554` | `bcce3917f6a06ff95076d3e5c048139c41e4c333` | `b51585141bb4495991bf38b0752911640bda68a0597bb6a9ec809b9582f7e141` |
| Karoshi Mario | `e94f7cb844007258d7379fcb0fe1c30a7a367203acb2a567bd942b1a9b0c268f` | `9efbd103e7f049fa21682c4d856f91577e111960` | `d72df9ac81a018093f455a3437df26d50ade243b96f55c09e7538ea066d9bc39` |

## Observed family

Both ROMs contain the active `4c 4d 00 01` (`LM 00 01`) marker at logical `$0FB604`. The settings
runtime points to an exact `$6D00` RATS payload:

| Oracle | RATS header | Payload | Runtime base operand |
| --- | ---: | ---: | ---: |
| Castle of WTF | `$080034` | `$08003C` | `$10803C` |
| Karoshi Mario | `$08017C` | `$080184` | `$108184` |

The payload is a `$2D00` all-`FF` prefix followed by 512 32-byte records; it has no later eight
special records. After masking the six authenticated allocation-dependent operand groups, every
immutable settings runtime and hook digest is byte-identical to the retained Lunar Magic 2.22
generation-1.01 family. This independently confirms that the `$0100`/`$0101` settings runtime is
one family and that only the active legacy-graphics marker distinguishes these generations.

Ghidra 12.1.2 on the labeled Lunar Magic 3.63 executable supplies the conversion decision:
`LoadLevelHeaderRecordWithVersionUpgrade` calls `CheckLegacyGraphicsTablePatchState`; a recognized
legacy `LM` generation dispatches `NormalizeLevelHeaderRecordReferences`. Disassembly at
`$00460A09` compares the marker prefix with `$4D4C`, and `$00460A1A` performs the generation
threshold comparison. Therefore generation 1.00 uses `LegacyReferenceLayout`, exactly like 1.01:
normalize reference words 2 through 10 except word 8, preserve the other legacy fields, and
initialize the eight absent current special slots.

## Verification

`external_generation_100_oracles_migrate_when_supplied` accepts the two paths through
`LM_EXPANDED_SETTINGS_100_ROMS`. For each authentic ROM it proves marker/runtime/RATS
authentication, 512-record conversion, eight-slot initialization, corruption rejection, atomic
replacement, current-runtime reopen, and byte-exact Undo.

The dense Castle of WTF allocation map also forced the new `$6E00` owner to an irregular address.
That run exposed a previously incomplete low-word relocation at runtime block `$172:+$16`.
`irregular_relocation_updates_the_complete_table_low_word` now locks the full 16-bit fixup rather
than the formerly insufficient low-byte-only fixup.
