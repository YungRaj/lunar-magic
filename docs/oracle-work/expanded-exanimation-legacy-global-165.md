# Expanded-ExAnimation legacy-global 1.65 oracle

This note records the non-redistributed historical before/after evidence used by
`external_lunar_magic_165_global_exanimations_migrate_reciprocally_when_supplied`.

## Source and capture

The unlocked **A CamTheMan Christmas** build dated 2009-12-09 was discovered in the Internet
Archive `SMWHG` historical-hack collection. Its legacy runtime matches the ExAnimation format used
by Lunar Magic 1.65, the final release before the complete 1.70 rewrite documented in Lunar
Magic 3.63's `readme.txt`. The archived game image and derived ROMs remain outside this repository.

Source collection: `https://archive.org/details/SMWHG`

| Artifact | SHA-1 | SHA-256 |
| --- | --- | --- |
| Source `.7z` | `5804df0439823b37da436faece55a9c8c5561774` | `c07d2d42ea101420ba7d231502f1000f0ea3eeebba7fb20a9f1db6c90b4a8e3a` |
| Legacy before ROM | `95123e2cdb0d3708071161c8fee22980fc377622` | `b8a593cff2f3ed04d326fd324be81af6e6c5db492c950e58a8e8133ce0336c6f` |
| Lunar Magic 3.63 after ROM | `2f8e8c645b011dd1f7852b6e91a2ee2b0f5c2c5d` | `a24584c730b19c3cfd10d1a34298a5add5f1a15c5b35c58b22799e69139dc74a` |
| Reciprocal level-105 MWL | `c903bdcba6524286ae28eb9dbfe027255aebd8f0` | `19fe9a788e2e9899776badc2900fb1d3321d7999682012db1df347aa04a25cb1` |

Lunar Magic 3.63 first exported level `$105`, then imported that same MWL into a byte-identical copy
of the before ROM. This invokes `EnsureExpandedExAnimationRuntimeInstalled` through the ordinary
level-save path without inventing different level content. Both ROMs are copier-headered and
2,097,664 bytes.

## Recovered legacy layout

The before ROM selects the legacy-global coordinator branch with JSL opcodes at logical `$02418`
and `$283AD`. The `$02418` operand selects the `$140` auxiliary allocation at `$086D10..$086E50`.
The runtime entry `$086E10` is intentionally inside that allocation, not at its payload start; its
`+$1A` operand selects the separate `$600` pointer table at `$08728D..$08788D`. Four of its 512
slots are populated.

This authentic layout corrected two overly narrow Rust assumptions:

- a legacy runtime must be fully contained in one valid RATS owner, but need not begin at the
  owner's payload start, and its prefix may intentionally overlap the auxiliary table;
- migration must replace the authenticated legacy fixed-hook family using source-snapshotted
  transaction preconditions instead of requiring pristine Nintendo bytes.

The Lunar Magic 3.63 result places the current `$C30` core at `$0A5D51` and its pointer table at
`$0A6989`. This irregular placement exposed a third bug: all twelve core IRAM words are
allocation-relative, using the runtime low word plus `$05AF`, `$05AF`, `$0B4A`, `$05CF`, then
`$0B82` eight times. They are now `Low16` relocation fixups rather than constants that happened to
match the usual `$080549` allocation.

## Verification

The opt-in reciprocal test accepts the paths through
`LM_EXPANDED_EXANIMATION_LEGACY_GLOBAL_BEFORE` and
`LM_EXPANDED_EXANIMATION_LEGACY_GLOBAL_AFTER`. It requires the authentic legacy/current probes,
runs the real application installation command, compares all 512 slots with Lunar Magic's result
(including all four populated animations), verifies the current runtime and checksum, and restores
the complete original physical ROM with one Undo.

`irregular_core_allocation_relocates_all_twelve_iram_words` independently forces a clean install to
logical `$0A0008`, checks every relocated word, strictly reopens the runtime, and undoes to the
byte-exact pristine ROM without depending on the external oracle.

The separate Lunar Magic 1.70 pointer-hook branch is authenticated by
`expanded-exanimation-legacy-pointer-170.md`.
