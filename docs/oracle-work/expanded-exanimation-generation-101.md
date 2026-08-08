# Expanded-ExAnimation generation 1.01 oracle

This note records the non-redistributed historical evidence used by
`detect_smw_us_v1_current_expanded_exanimation_runtime`. ROM images and patches remain outside the
repository.

## Sources

Three independently authored historical hacks were examined:

- Karoshi Mario (Lunar Magic 1.71):
  `https://smwdb.me/db/8/8fa669d86edce50e6d61ba2855817ffe59dc2e30/`
- Super Mario World 2 Player Co-op (Lunar Magic 1.82):
  `https://smwdb.me/db/9/9b7465aadd656b0c93d347b0c60ca74799788eeb/`
- SMB3 Raccoon Mario (Lunar Magic 1.91):
  `https://smwdb.me/db/f/fe0aa48dc19162d969d11b74ce4bcbe938a5a997/`

Only their IPS patches were retained and applied to the authorized local headered SMW-US
revision-0 fixture. The resulting ROMs are test inputs supplied outside the repository.

| Oracle | IPS SHA-256 | Patched ROM SHA-1 | Patched ROM SHA-256 |
| --- | --- | --- | --- |
| Karoshi Mario | `e94f7cb844007258d7379fcb0fe1c30a7a367203acb2a567bd942b1a9b0c268f` | `9efbd103e7f049fa21682c4d856f91577e111960` | `d72df9ac81a018093f455a3437df26d50ade243b96f55c09e7538ea066d9bc39` |
| 2 Player Co-op | `7b18788db40b8b539ec963f220a6c340b5df362f8e4f70f28cabe57016d2140d` | `51d87c2f683cbcdb346be17a2730f4a25f9e0336` | `23e71a699f3040ef504fc3c680374539b78e570ff7e679c6a8e000845891b0e9` |
| Raccoon Mario | `9febe937b69b5825029a065b860f32a3ca2da85dd1930effff41dd46e8530aa6` | `5874045562967a51d66ec76c1ad953f23f516760` | `ab4fc91577d390bd3899675d41586611346d7e6353c0468b8cbcd5e4352fd076` |

## Observed family

All three ROMs contain a `$C30` RATS-owned core with marker `4c 4d 01 01` (`LM 01 01`) at
core `+$169`. After canonicalizing only the two mapper bytes, four mutable feature bytes, eight
SNES-pointer operands, twelve IRAM words, and 108 allocation-relative local words, all three cores
have the same SHA-256 digest:

`10dbd77a94ddb479cf4ac83360c824ccf97f58dbbc55f61770465369e6cc90e0`

The twelve historical IRAM words are the mapped runtime low word plus `$05AF`, `$05AF`, `$0B4A`,
`$05CF`, then `$0B82` eight times. The pointer and local-word relationships match the current
runtime. Generation 1.01 has its own authenticated shared-palette helper payloads, and a fixed
shared hook may use the equivalent high-bank LoROM mirror.

The Co-op ROM retains the complete unmodified runtime family, including both allocated helpers,
and therefore supplies the strict end-to-end detector oracle. Karoshi and Raccoon contain later
hack-owned changes to fixed helper locations and correctly fail complete-family authentication;
their identical canonical core digest supplies independent core evidence without allowing those
modified fixed ranges. Karoshi additionally supplies populated per-level ExAnimation records.

## Verification

`external_generation_101_current_family_authenticates_when_supplied` uses
`LM_EXPANDED_EXANIMATION_101_ROM` to prove the complete Co-op family, current-generation probe,
RATS ownership, immutable-core and IRAM corruption rejection, and owner corruption rejection.

`external_generation_101_level_payloads_round_trip_when_supplied` uses
`LM_EXPANDED_EXANIMATION_101_PAYLOAD_ROM` to traverse all 512 pointer slots in Karoshi. It derives
the observed one/two-word frame widths consistently from each tagged compact payload, decodes every
populated animation through the production project loader, and semantically re-encodes and decodes
each value without loss.

This evidence broadens the authenticated current-family variants. The two separate legacy
migration branches are authenticated by `expanded-exanimation-legacy-global-165.md` and
`expanded-exanimation-legacy-pointer-170.md`.
