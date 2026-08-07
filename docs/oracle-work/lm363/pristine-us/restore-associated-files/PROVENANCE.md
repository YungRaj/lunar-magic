# Lunar Magic 3.63 associated-file restore archive

`all-sidecars.lrp` was created live by an isolated 32-bit Lunar Magic 3.63 process under Wine. The
source was the retained headered pristine SMW-US ROM, copied to `oracle.smc`; the matching
`sysLMRestore/smwOrig.smc` contained the same pristine image.

## Inputs

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- pristine headered SMW-US SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- `all-sidecars.lrp` SHA-256:
  `11281c65ba9460308a17a5ac6985ae2e036db706c0f2dc74e988f08ab781d162`
- archive length: 3,060 bytes

All thirteen ROM-adjacent slots were populated before archive creation. Compact tracked files were
copied byte-for-byte under Lunar Magic's required basename and extensions:

| Slot | Extension | Source | SHA-256 |
|---:|---|---|---|
| 0 | `msc` | `crates/lm-rom/Cargo.toml` | `28942d39263a6e862cc8710023748c7ee1fbcee08ff1a455e304429a8b959953` |
| 1 | `dsc` | `crates/lm-snes/Cargo.toml` | `a2c9bd324c1edfe3f86db1d65acc3e5ae2d3fd169ef0e04bf3c063f3ef78bfff` |
| 2 | `ssc` | `crates/lm-rats/Cargo.toml` | `b2fac796eea73e05ba1e484bad15d7462e62ea24c9280a648e4122067803fb1f` |
| 3 | `m16` | `crates/lm-codec/Cargo.toml` | `810aeea163823ae6582f0b38d5d4717404f8b77d264daaac98b112eb21b91eac` |
| 4 | `s16` | `crates/lm-level/Cargo.toml` | `126f4c677967bf28af24725abb9041902759ab3f51d7318b586ca3490d02de5f` |
| 5 | `mwt` | `crates/lm-graphics/Cargo.toml` | `558dc46ce8e731a519d1c14ae8ec3f59b0aca91f7b35925f0abed7674b182daa` |
| 6 | `mw2` | `crates/lm-title/Cargo.toml` | `0f04c53c262c85e9d3bb4a6893f6627ec9ac2b49dd126c495dec947742fa6b52` |
| 7 | `sscov` | `crates/lm-overworld/Cargo.toml` | `d89a2e7b549b566efec5d29b92576e68a58d2efc35b6bdd493e1497811145908` |
| 8 | `s16ov` | `crates/lm-package/Cargo.toml` | `5bdb57021ecf19e2facd838303fef3b698d45f6f28f8d04779106ae3a5121775` |
| 9 | `lmtbl` | `crates/lm-project/Cargo.toml` | `9e241c6578a3c4ceefa1474f7abdf6884e84cbe4c3aead2ebf497474bf90b633` |
| 10 | `mw0t` | `crates/lm-render/Cargo.toml` | `413076989066c3a1475d4e9f653f1961ec0521cb68b0c63ddde25653c9af5eda` |
| 11 | `mw0` | `crates/lm-oracle/Cargo.toml` | `7e8c2dd90d0b415c8d5451a125be5769ad9263f0143ccf552a557aae0ee7d488` |
| 12 | `osc` | `crates/lm-profile/Cargo.toml` | `17cb393c7494b377fd5e95ab508b7fc97b9cb70ba3b26a01dcda39112bbe170e` |

## Original workflow

The ROM was opened through Lunar Magic's native file dialog. Command `$24CE` opened `Restore Point
Options`; its live controls showed all of the following enabled, including `Include auxiliary ROM
files in restore points (.msc, .dsc, etc)` and `Use compression when creating new restore points`.
Command `$23B8` then opened `Create Full Restore Point`; the accepted description was
`All thirteen compact sidecars`. Lunar Magic published `sysLMRestore/oracle.lrp` without an error
prompt.

## Original restore dialog

The same isolated setup then drove command `$23B9`, `Restore ROM to Previous State`, using the
retained archive. The original dialog populated one owner-drawn row with record ID 1, initially had
no row selected, and defaulted `Restore auxiliary ROM files if present in restore points (.msc,
.dsc, etc)` on. Selecting row 0 and pressing OK displayed title `WARNING!` and body `THIS WILL
OVERWRITE YOUR CURRENT ROM AND AUXILIARY FILES!!`, followed by a blank line and `Proceed?`.

Choosing No kept the dialog open with row 0 selected and left both the complete ROM and archive
SHA-256 unchanged. Choosing Yes closed the open ROM, restored it byte-exactly, published all
thirteen sidecars with the hashes above, and appended Lunar Magic's successful reversion record:
the archive grew from 3,060 to 3,343 bytes and changed from SHA-256 `11281c65...781d162` to
`96af6d8c...f70e0`. `dialog-oracle.tsv` retains the exact complete hashes and observed controls.
`tools/wine-restore-dialog-oracle.c` performs deterministic record inspection and selection without
screen-coordinate assumptions.

## Rust verification

`authentic_lunar_magic_archive_restores_all_thirteen_associated_files` embeds this exact archive,
validates its header, linked directory, stored checksum, compressed payloads, description, and
record count, then reconstructs every associated slot through the public Rust reader. Each restored
byte vector is compared with its immutable capture-time length and CRC-32, while the table above
retains its SHA-256. Thus later edits to the source Cargo manifests cannot invalidate the authentic
fixture, while slot order, extension identity, compression boundaries, stored ranges, and decoded
contents remain covered by the normal test suite rather than only by this manifest.
