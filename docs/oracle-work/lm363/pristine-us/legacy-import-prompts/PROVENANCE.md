# Lunar Magic 3.63 legacy-import prompt observations

These observations were captured live from an isolated 32-bit `Lunar Magic.exe` 3.63 process under
Wine. The process opened the authenticated `palette-install-positive/after.smc` ROM. Command
`0x238D` opened a level file, and the native file dialog received each fixture path through control
`0x047C`. Top-level dialog titles and child-static text were enumerated directly from the owning
process before accepting the prompt.

## Declared custom palette with missing `.mw3`

Input was the retained `legacy-level-000-custom-palette` five-file bundle with only `Level 000.mw3`
omitted.

```text
title: File Missing!
message: Couldn't locate the palette file! Switching to non-custom shared palette.
button: OK
```

After accepting the prompt, a legacy re-export began its Layer 1 row with flag `00` instead of the
input's `01` and emitted no `.mw3`. This proves the importer continues with the destination shared
palette and clears the custom-palette flag; it does not reject the level.

## Missing required Layer 1 sidecar

Input was the retained `legacy-level-105` bundle with only `Level 105.mw0` omitted.

```text
title: Couldn't open file!
message: Level 105.mw0
button: OK
```

The level was not imported. This distinguishes the optional palette fallback from required Layer 1,
Layer 2, and sprite sidecar failures.

## Present short and overlong palette sidecars

A two-byte `.mw3` containing `00 00` imported without a prompt. Re-export emitted a complete
514-byte `.mw3`: the two supplied bytes replaced the beginning of the destination palette buffer
and the remaining 512 bytes retained the destination level's palette. This includes byte-granular
behavior, so an odd final source byte replaces only half of one SNES color word.

A 515-byte `.mw3` made from the retained authentic payload plus byte `AA` also imported without a
prompt. Re-export was byte-identical to the original 514-byte authentic payload, proving trailing
bytes are ignored.

```text
96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7  short input mw3
8a50127cc38c0f39120687e3b4c2fa3067ded7dfbddf49c88a1d431003640c8f  short re-export mw3
71071d634be0815a7a74fef8fd973091a532db038355b220a6b6a5f654c6107b  overlong input mw3
43c981cb1409b77459907a2d18d401796eee85f3fe29de030123a10d9afa7a07  overlong re-export mw3
```

## Present short Layer 2 tilemap sidecar

For level 000, the authentic 2,048-byte `.mw1` was replaced by only its first two bytes (`F1 00`).
The file imported without a prompt. A legacy re-export emitted a complete 2,048-byte `.mw1` whose
first two bytes remained `F1 00` and whose remaining 2,046 bytes were zero. This proves that the
legacy importer clears its fixed Layer 2 tilemap workspace, performs a partial read into its
prefix, and retains zeroes for the unread suffix. It does not reject short background sidecars or
merge their suffix with the destination level's background.

```text
079950a589d9712d69c276de82c668764cb30dc8940d56fe01d076b333df29b7  two-byte input mw1
ec8db8ae218504df46a1e6c7b1dc1f6d2a55129f2d836e950e81f22f04281628  2,048-byte re-export mw1
```

An authentic 2,048-byte `.mw1` extended by one zero byte also imported without a prompt. Its
re-export was byte-identical to the authentic 2,048-byte payload, proving that the same fixed
workspace read ignores trailing Layer 2 bytes.

```text
f4bb1b6429b9920e4c59d3f9eb5e58b3f2d0e80aeebf49e1dc845ec45124bf2d  2,049-byte input mw1
67cb940c874127ebca7fdaf0da44e1f683c5040fd36cb746b5222ffd055cfffe  2,048-byte re-export mw1
```

## Layer 1 terminator boundaries

An authentic 33-byte `.mw0` extended by one zero byte imported without a prompt. Its re-export was
the original 33-byte stream, proving bytes after the first `FF` object-stream terminator are
ignored.

Removing only that final `FF` also imported without a prompt. Re-export restored the missing
terminator and was byte-identical to the authentic stream. The Rust compatibility path therefore
supplies a terminator only after parsing reaches the clean end of complete records; it continues
to reject a partial final object record.

```text
4d625c7585d008a646542e01c61baba9fe0b38cb914c279adf34534b8bbd7ca7  34-byte trailing-data input mw0
176cb452f3b71524839f1620f3ab44231e00ae9b5328844f653e4d6e2aab84b7  32-byte unterminated input mw0
38a203f968425a74bce6345426419a5f01e66eb9c5808423f64ab04e088199a8  canonical 33-byte re-export mw0
```
