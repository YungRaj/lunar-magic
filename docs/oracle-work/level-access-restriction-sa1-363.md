# SA-1 level-access restriction oracle — Lunar Magic 3.63

This oracle covers Lunar Magic's permanent level-access restriction command on an SMW-US ROM
with SA-1 Pack v1.40 installed. The copyrighted ROM images remain outside the repository.

## Provenance

- Lunar Magic 3.63 executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Official SA-1 Pack v1.40 BPS SHA-256:
  `20db9cbc8c21b3c081a9ad3b1e68ee4453d107c062304460fa25e0f6537ac84c`
- Authenticated installed SA-1 source ROM SHA-256:
  `827f396152867cf296b4e481d916cebd432ae616fee392e488f5828b91cc226d`
- Lunar Magic restricted output SHA-256:
  `fd0c016b4ac94849ac1b8b3e546d0c8a80cc87ec23dde3716a8f68357ae78604`
- Physical size: `$100200` bytes, including the retained `$200`-byte copier header
- Title: `Codex Parity Test`
- Per-save low/high keys: `$48/$16`
- Graphics key: `$4DC8`
- Complete-ROM mutation shape: 29 disjoint physical ranges

The source was produced by applying the official `sa1.asm` with Asar 1.91 to the authenticated
Lunar Magic-installed SMW source. Lunar Magic 3.63 then executed command `$23A4` in a fresh,
unmodified isolated Wine process. The IPS offer was declined after the ROM mutation completed.
The stored checksum remained the valid original `$D9B0` value.

## Recovered behavior

Port-8089 Ghidra recovery of `BuildMappedRomLayoutDescriptor` (`0047B550`),
`ValidateAndInitializeOpenedRom` (`0047C120`), `RestrictLevelAccessInRom` (`004849B0`), and
`HandleRestrictLevelAccessCommand` (`00485050`) establishes that map mode `$23` selects SA-1
address translation while retaining the base descriptor's lower-four-megabyte physical offsets.
Unlike ExLoROM, SA-1 does not take either metadata-mirror branch.

The original bulk resave also upgrades seven guarded SA-1-owned byte regions before applying the
ordinary restriction family. Rust authenticates every source region before writing any of them,
then reproduces that runtime upgrade, the base hooks and protected data, the non-mirrored title and
version metadata, and the SA-1 checksum-compensation run. A mismatch in any prerequisite rejects
the complete transaction without changing ROM bytes or history.

## Exact gate

```text
LM_SA1_RESTRICTION_BEFORE=... \
LM_SA1_RESTRICTION_AFTER=... \
cargo test -p lm-profile \
  level_access_restriction::tests::sa1_restriction_matches_authentic_lunar_magic_output_exactly \
  -- --ignored --exact

1 passed; 0 failed
```

The assertion compares the entire physical ROM, then proves Undo restores the exact source and
Redo restores the exact Lunar Magic output. Portable tests independently bind the SA-1 application
route, descriptor offsets, seven guarded prerequisites, compensation bounds, and validate-all-
before-write atomicity.
