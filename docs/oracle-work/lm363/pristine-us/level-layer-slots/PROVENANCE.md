# Lunar Magic 3.63 level-layer slot oracle

`slot-arrays.tsv` was captured from a live 32-bit Lunar Magic 3.63 process under Wine with:

```text
tools/lunar-magic-layer-slot-audit.sh \
  docs/oracle-work/lm363/pristine-us/level-layer-slots/slot-arrays.tsv
```

The audit opens the retained installed SMW-US ROM, reads Lunar Magic's three initialized 32-byte
mode tables, and invokes `ConfigureLevelLayerSlotAssignments @ 004692B0` atomically for every valid
mode (`$00..$11`, `$1E`, `$1F`) with legacy Layer 3 priority splitting both off and on. Each pair
includes the unmodified table state plus all four combinations of expanded packed bit 31's source
route and bit 30's primary additive input, for 200 cases total. Each row retains the five source,
enabled, additive, half-color, and Layer 3 priority bytes.

Inputs:

- `Lunar Magic.exe` SHA-256: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- installed ROM SHA-256: `9363981e0e902b00336184d9f2307773631a3b2bb89524cd4fb60c8c9db53882`
- `slot-arrays.tsv` SHA-256: `f810831254de21697b3a4519ba65c3f9c45128ad52334edb01d65989510ad8a8`
