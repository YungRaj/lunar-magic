# Lunar Magic 3.63 credits transfer oracle

This fixture was produced locally from the user-supplied Lunar Magic 3.63 executable and headered
US Super Mario World ROM. The source and destination began as separate byte-identical copies of
`oracle-work/lm363/pristine-us/headered.smc`.

The noninteractive operation is documented in Lunar Magic's bundled help and was invoked through
Wine as:

```text
Lunar Magic.exe -TransferCredits dest.smc source.smc
```

Lunar Magic reported success. It preserved the source, expanded the destination from `0x80200` to
`0x100200` file bytes, installed one RATS block at logical `$80000` with payload
`$80008..$80759`, and produced these SHA-256 digests:

- `before.smc`: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- `after.smc`: `7fbf52b33774bcb4c7fac9480728c955caf8c454313308f29fabdc9c1e4ee34e`

The Rust decoders independently export both images to the same canonical 16,392-byte `LMCREDT1`
file and the same row-addressable `LMOBS1` observation. This proves that Lunar Magic's first-time
expanded runtime installation preserves all 8,192 materialized credits words, including the 54
blank rows absent from pristine 202-row storage.

`oracle.manifest` records the complete physical transition and is replayed with:

```text
lm-cli oracle-verify oracle.manifest before.smc after.smc before.obs after.obs
```
