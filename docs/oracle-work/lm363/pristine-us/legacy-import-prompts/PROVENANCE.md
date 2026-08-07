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
