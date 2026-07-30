# Lunar Magic 3.63 title-screen transfer oracle

This fixture was produced locally from the user-supplied Lunar Magic 3.63 executable and
headered US Super Mario World ROM. The source and destination began as separate byte-identical
copies of `oracle-work/lm363/pristine-us/headered.smc`.

The noninteractive operation is documented in Lunar Magic's bundled help and was invoked through
Wine as:

```text
Lunar Magic.exe -TransferTitleScreen dest.smc source.smc
```

Lunar Magic reported success. It preserved the source, expanded the destination from `0x80200` to
`0x100200` file bytes, installed one RATS block at logical `$80000` with payload
`$80008..$8074D`, and produced these SHA-256 digests:

- `before.smc`: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- `after.smc`: `37081733ff0f2120be0d89be33b95b41199ef110c27269b61a6649f8209037f5`

The Rust decoders independently export both images to the same canonical `LMOWLYR1` file and
`LMOBS1` observation. This comparison exposed and now guards Lunar Magic's normalization of 518
untouched primary-plane blank words from `$00FC` to `$38FC`; the blank secondary plane remains
`$00FC`.

`oracle.manifest` records the complete physical transition and is replayed with:

```text
lm-cli oracle-verify oracle.manifest before.smc after.smc before.obs after.obs
```
