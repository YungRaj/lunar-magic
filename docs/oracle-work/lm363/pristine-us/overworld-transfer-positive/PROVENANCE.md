# Lunar Magic 3.63 overworld transfer oracle

This fixture was produced locally from the user-supplied Lunar Magic 3.63 executable and headered
US Super Mario World ROM. The source and destination began as separate byte-identical copies of
`oracle-work/lm363/pristine-us/headered.smc`.

The noninteractive operation is documented in Lunar Magic's bundled help and was invoked through
Wine as:

```text
Lunar Magic.exe -TransferOverworld dest.smc source.smc
```

Lunar Magic reported success. It preserved the source, expanded the destination from `0x80200` to
`0x100200` file bytes, added 23 valid RATS allocations, and produced these SHA-256 digests:

- `before.smc`: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- `after.smc`: `f1c17198bd5193783f32a183afb7b998c0e2547a75be9546f929ddeefaa296a6`

This first aggregate-transfer observation covers the main event-reveal workspace. Lunar Magic
leaves the destination operand on its fixed table but relocates the source operand to a `$F0`-byte
RATS payload. Its loader consequently materializes 120 records instead of the pristine 112:
the original 112 source words are preserved and eight additional source words are zero. The
destination bytes are read through the still-fixed pointer for the same RATS-derived length.

The canonical `LMOWEVT1` files and entry-addressable `LMOBS1` observations preserve this exact
behavior. The original `oracle.manifest` remains the narrowly scoped main-reveal record.
`oracle-events.manifest` and `before-events.obs`/`after-events.obs` bind all four recovered event
domains over the same 124 changed ranges and 23 new RATS owners: 120 main reveals, the unchanged
96-entry event-number map, the unchanged 24 special-event records, and the installed LZ2 event
tilemaps materialized from the pristine legacy representation. The latter contains 92 nonzero
event indexes, 74 nonzero auxiliary bytes, and a zero secondary-high plane.
`oracle-full.manifest` and `before-full.obs`/`after-full.obs` extend that proof to thirteen
recovered native domains. The transfer preserves all 8,192 Map16 definition words, 2,884
normalized acts-like entries, 14 path links, two player starts, seven overworld
settings records, 194 ordinary messages, and seven boss-sequence messages. It materializes 54
warp links from the pristine 27-link representation and 96 direct level names from the pristine
93-name representation. All three manifests intentionally retain the same 124 changed ranges and
23 newly owned RATS blocks while differing only in their semantic observation scope.

Replay with:

```text
lm-cli oracle-verify oracle.manifest before.smc after.smc before.obs after.obs
lm-cli oracle-verify oracle-full.manifest before.smc after.smc before-full.obs after-full.obs
```
