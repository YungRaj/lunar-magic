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
