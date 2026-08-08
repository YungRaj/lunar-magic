# Expanded-ExAnimation legacy-pointer 1.70 oracle

This note records the non-redistributed historical before/after evidence used by
`external_lunar_magic_170_pointer_hooks_migrate_reciprocally_when_supplied`.

## Source and capture

The unlocked **Super Mario World - The Lost Levels for Super Players! (Demo 2)** build dated
2010-04-06 was discovered in the Internet Archive `SMWHG` historical-hack collection. It was
published between Lunar Magic 1.70 (2010-04-01) and 1.71 (2010-04-17) and contains the exact
pointer-hook generation selected by `PatchLegacyExAnimationPointerHooks`.

Source collection: `https://archive.org/details/SMWHG`

| Artifact | SHA-1 | SHA-256 |
| --- | --- | --- |
| Source `.7z` | `c5f2de8b6e74649de21bdb7b4976d9616e28ef26` | `088981546bffe8ac6491096910007161f21f497f8c86b40a3893a7c1bcb66ef5` |
| Legacy-pointer before ROM | `32c42ec14afbd3edb03ba2b27cc65139257691ac` | `b092791fd410cdfb397a75aa056eef7e7e099476340cbe6a0f355a8165b0335a` |
| Lunar Magic 3.63 after ROM | `915402c956f9ff58575d0b6afb3bcf87e156dbc8` | `3d5e4334b5a51cc1ea87582849b86ffc144322998f9948355a2327421884963f` |

Both derived ROMs remain outside the repository. They are copier-headered and 2,097,664 bytes.
The before image was first passed through a reciprocal level-`$105` save so unrelated historical
level formats were already upgraded while the legacy ExAnimation marker remained intact. Lunar
Magic 3.63 then opened that baseline in an isolated Wine prefix. Command `$2530` opened the real
level ExAnimation editor, one valid slot was inserted, and command `$2392` saved the level. This
actual dirty ExAnimation save invokes the pointer-hook migration; merely opening the dialog or
round-tripping a level without changed ExAnimation data does not.

## Observed migration

The fixed JSL at logical `$0283AD` resolves the RATS-owned `$C30` runtime at logical `$0A0BA0`.
The authenticated source marker is `4C 4D 00 01` at runtime `+$169`. During the same save that
publishes the edited animation, Lunar Magic changes exactly the three recovered pointer-hook
values:

- runtime `+$92`: `$00` to `$10`;
- runtime `+$118`: `$00` to `$10`;
- runtime `+$16B`: generation byte `$00` to `$01`, producing marker `4C 4D 01 01`.

The complete before/after save has 1,295 changed bytes in 66 ranges because it also serializes the
deliberately inserted level animation. The reciprocal test therefore compares the complete three
fragment write surface owned by `PatchLegacyExAnimationPointerHooks`, rather than falsely
attributing the animation payload allocation to that helper.

## Verification

The opt-in test accepts the paths through
`LM_EXPANDED_EXANIMATION_LEGACY_POINTER_BEFORE` and
`LM_EXPANDED_EXANIMATION_LEGACY_POINTER_AFTER`. It requires the authentic before image to classify
as `LegacyPointerHooks`, constructs the production migration plan, compares all three guarded Rust
fragments with Lunar Magic 3.63's output, applies the real application command, verifies checksum
repair, and restores the complete original physical ROM with one Undo.

The historical hack changes an unrelated fixed current-runtime helper at logical `$077550`, so its
post-migration image is intentionally not used as a strict complete-current-family oracle. The
independent Lunar Magic 1.71/1.82/1.91 fixtures authenticate that family; this fixture proves only
the formerly open 1.70 pointer-hook transition.
