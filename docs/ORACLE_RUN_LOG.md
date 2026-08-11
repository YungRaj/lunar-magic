# Original Lunar Magic oracle run log

This log records locally executed, opt-in original-editor gates. These tests stay ignored in the
default portable suite because they require a legally supplied Lunar Magic executable, SMW ROM,
Wine, and a graphical user session. A passing entry means the named test completed its full
original-editor import/export and semantic comparison; process launch alone is not counted.

## 2026-08-05 — Lunar Magic 3.63 core level-editing audit

- Lunar Magic executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Pristine SMW-US ROM SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- Runtime: `wine-11.13 (Staging)` on macOS/Apple Silicon

Commands and outcomes:

```text
cargo test -p lm-app --test level_header_wine -- --ignored --nocapture
2 passed: lunar_magic_exports_every_rust_legacy_level_header_field;
lunar_magic_canonicalizes_every_reserved_mode_without_losing_background_color

cargo test -p lm-app --test expanded_level_mode_wine -- --ignored --nocapture
1 passed: lunar_magic_exports_rust_persisted_expanded_level_mode

cargo test -p lm-app --test exanimation_features_wine -- --ignored --nocapture
1 passed: lunar_magic_exports_rust_persisted_animation_feature_options

cargo test -p lm-app --test support_patch_b_wine -- --ignored --nocapture
1 passed: lunar_magic_preserves_rust_installed_support_patch_b_and_custom_time

cargo test -p lm-app --test sprite_growth_wine -- --ignored --nocapture
18 passed; 0 failed
```

The 18-test level suite covers semantic MWL object and sprite edits, direct sprite growth,
legacy/expanded sprite ordering, expanded-control canonicalization, expanded-framing retention and
downgrade, vertical expanded ordering, Layer 1 control and extent canonicalization, raw Layer 1
ordering, all screen-exit boundary shapes, direct and packed entrances, all eight vertical-range and
Smart Spawn combinations with preservation of their five shared flags, existing separate-midway
updates, and first-install separate-midway runtime publication. The exhaustive spawn test also
reopened a checksum-valid ROM after all sixteen original-editor import/export operations.

## 2026-08-05 — Layer 2 and Layer 3 publication audit

The executable, pristine ROM, and Wine identities are unchanged from the core audit above.

```text
cargo test -p lm-app --test layer2_wine -- --ignored --nocapture
2 passed: checksum-atomic tilemap editing and semantic object-backed editing

cargo test -p lm-app --test layer3_install_wine -- --ignored --nocapture
1 passed: first-time Rust Layer 3 installation reopened and exported canonically in Lunar Magic
```

The object-backed Layer 2 oracle edits through the same semantic relocation operation used by the
native canvas. This regenerates owned screen transitions, including removal of a redundant leading
screen-zero jump, before Lunar Magic re-exports the exact expected payload.

## 2026-08-05 — Secondary-exit boundary and clear audit

The executable, pristine ROM, and Wine identities are unchanged from the core audit above.

```text
cargo test -p lm-app --test secondary_exit_wine \
  lunar_magic_imports_reexports_and_clears_secondary_exit_boundaries \
  -- --ignored --exact --nocapture
1 passed; 0 failed; latest expanded canonicalization run finished in 44.41s
```

The gate imported records at both valid table endpoints (`$0000` and `$1FFF`) with minimum and
maximum packed fields. Its source also contains a duplicate `$1FFF` record, nonzero byte-7 values,
and an invalid `$2000` record; Lunar Magic must retain the last valid duplicate, clear byte 7, and
skip the invalid key. The test independently reopens both installed ROM-table entries through Rust.
It then imports an empty secondary-exit set, requires Lunar Magic to export an empty set, reopens
both endpoint entries as native zero records, and verifies the final ROM checksum.

## 2026-08-07 — Shared undo-history configuration audit

The executable and pristine ROM hashes match the core audit above. Each original-editor launch used
an isolated Wine prefix and a unique process name so another Lunar Magic session could not satisfy
the live-memory observations.

```text
cargo test -p lm-app --test undo_history_wine \
  original_lunar_magic_shares_and_clamps_every_undo_history_boundary \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 121.16s
```

The gate proves a fresh prefix applies 33 snapshots to both the level and overworld editors. It then
sets `UndoMain` to 0, 1, 2, 9, 33, 51, and 52 and reads both original live effective-limit globals.
Both editors retain every in-range value and independently clamp 52 to 51. Ghidra supplies the
complementary baseline-counting, disabled-capture, pruning, and reset control-flow evidence.

## 2026-08-08 — Map16 Popularity bitmap-import differential audit

- Lunar Magic executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Expanded SMW-US working ROM SHA-256:
  `73003400046213cfc0b9352a20e80682173d2b023e15e8a826f3dfeff3de81a4`
- 32×32 BMP source SHA-256:
  `7665fbb71678b38e73bfa36ac76428457d95e26f67e0ecdace57b4a97d72752b`

Three disposable-process captures used Popularity with a 16-color limit and independently selected
method 1 only, method 2 only, and neither neighborhood method. The original changed respectively
352, 351, and 374 graphics-cache bytes. The extended harness records controls `$6C` and `$6D` and
now reloads level `$105` correctly after discarding prior in-memory conversion state.

The ignored `map16_bitmap_wine_capture` differential gate reconstructs the active RGB32 palette
rows, live 128-byte entry-state map, and planar `$000–$2FF` graphics workspace from each capture.
All three captures now match exactly after recovering weighted RGB555 source mapping,
row-at-a-time exact allocation, direct occurrence tie weights, unassigned-subset aggregation,
direct per-color extension weights, lowest-lightness HSL run anchors, and the native
lightness/saturation distance weights. The no-neighborhood, method-1, and method-2 variants each
pass across both active palette rows and the complete graphics workspace. A Method 2 breakpoint
capture at `004f0269` additionally produced the same 4,096-byte pre-allocation color plane as Rust,
SHA-256 `07e0db077220846dd17b13b743718152779f79cd5d2034240c944483808fc8d9`.

A fourth capture enables Maintain Detail with Method 1. The first differential exposed one source
pixel incorrectly retained by Rust. A breakpoint at `$004F0269` showed Lunar Magic's reduced
candidate count is 17: the requested 16 opaque colors plus a leading zero-color sentinel. That sentinel
claims the nearest unused bitmap color during Maintain Detail's distinct-source pass. After adding
the sentinel to both native nearest mapping and the distinct-source allocator, the complete final
palette and `$000–$2FF` graphics workspace match exactly. A repeated original capture produced the
same palette, graphics, and entry-state buffers, ruling out timer or clipboard noise.

A fifth normalized capture clears control `$74`, keeps Maintain Detail clear, and restricts
conversion to the existing non-reserved palette words. Lunar Magic leaves row 0 byte-identical,
rebuilds row 1 from the weighted used colors, and emits graphics using that later row. The exact
differential recovered three coupled details: retained exact matches remain available without
blocking per-row extension, an equal-error row with exact movable assignments wins over a row that
only retains those words, and duplicate equal palette words choose the later entry in this mode.
After reproducing those rules, all 32 active palette words and the complete `$000–$2FF` graphics
workspace match byte-for-byte. Focused tests independently lock later-row rebuilding and the
duplicate-entry tie.

A subsequent Median Cut sweep covers maximum-color settings 1, 2, 4, 8, 9, and 16, plus a
Popularity maximum-one capture. Breakpoints at `004f0401` and `004f0769` recovered the previously
missing high-color tile-capacity pass. The live 16-color run reported a 12-entry free-row capacity;
tile `(8,16)` changed from 12 to 11 colors and tile `(16,16)` from 13 to 11 after the native
first-entry reservation, border-frequency boosts, stable strongest-color selection, and nearest
RGB555 remapping. The same run also proved that the quantizer receives source components already
rounded onto the SNES lattice, retains equal-axis cuts in blue/green/red order, and maps every one
of the 1,024 reduced pixels exactly. After implementing those boundaries, all eight retained
captures match Lunar Magic's first 32 active palette words and complete `$000–$2FF` graphics
workspace byte-for-byte. `captured_bitmap_components_produce_the_original_single_cluster_mean`,
`exact_capacity_tile_without_the_reusable_first_color_drops_its_last_weak_tie`, and the ignored
`lunar_magic_bitmap_capture_matches_rust_palette_and_graphics` differential bind the recovered
unit and end-to-end behavior.

## 2026-08-09 — Complete Map16 bitmap-definition workspace audit

- Lunar Magic executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Pristine SMW-US ROM SHA-256:
  `7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7`
- 32×32 BMP source SHA-256:
  `7665fbb71678b38e73bfa36ac76428457d95e26f67e0ecdace57b4a97d72752b`

The bitmap-import process harness now captures the complete 524,288-byte live Map16 definition
workspace at `00777e58` before and after conversion, in addition to the palette, entry-state, and
graphics planes. The ignored `map16_bitmap_wine_capture` gate reconstructs all 65,536 definitions
and requires exact equality across the complete post-import workspace. The live workspace is
column-major within each definition (`TL, BL, TR, BR`), unlike row-major ROM/file definitions.

Three disposable original-tool captures pass. Default deduplicated allocation at `$8200` changes
31 workspace bytes and produces post-image SHA-256
`8157e3a8704f84817166404cd7beddb2dc8171a9456454e31ecfeaf4013e0d51`. Clearing Optimize 16x16
also changes 31 bytes but places the 2×2 source spatially at `$8200/$8201/$8210/$8211`; its
post-image SHA-256 is `575e2c1e74f6d2ed72153dcfd217629c26f9cad9c968e2d0d74582791b51ff21`.
That capture exposed and corrected Rust's formerly flattened sequential placement. A third capture
enables layer priority and starts at nondefault cursor `$83A5`; its post-image SHA-256 is
`d5626f27e7737807600fd960142ba0e2ffb0e6b8e400846ae61b4e68d82fc892` and also matches exactly.

Direct assembly inspection at `004EF090` independently proves the sequential source is divided
into 16-column strips, each row starts at a destination stride of `$10`, and subsequent strips
advance by `source_height * $10`. The native session now also applies Lunar Magic's exact allocation
bound: `$8000` for a cursor below `$8000`, otherwise the next `$1000`-tile boundary. Foreground and
background save/reopen/undo tests pass, the renderer's 232-test suite is green, and the complete
512-slot pristine native-render dimension/empty-outcome gate passes after a clean rebuild.

## 2026-08-08 — Per-level palette transfer audit

- Lunar Magic executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Pristine headered SMW-US ROM SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`

```text
cargo test -p lm-app --test level_palette_transfer_wine -- --ignored --nocapture
1 passed; 0 failed; finished in 30.11s
```

The isolated Wine gate drives the recovered level-editor commands `$239F` and `$23A0` rather than
the palette editor's distinct shared-palette buttons. From pristine level `$105`, Lunar Magic 3.63
exports exact RGB `.pal`, version-2 TPL, and raw `.mw3` files with lengths `$300`, `$204`, and
`$202`; their retained SHA-256 identities are respectively
`88586ad377c5501476d93a820387c58312df9d05a64dd68af8f3131d71d10afa`,
`d4da32140cc2994b332e2bfd86579a7002868d692a4c6779ae99adedc6182201`, and
`8a50127cc38c0f39120687e3b4c2fa3067ded7dfbddf49c88a1d431003640c8f`.
The gate independently checks every RGB channel against the exported SNES words and proves the
row-zero/backdrop substitution across all sixteen rows.

The reciprocal phase changes one selected TPL word and one unselected word while supplying an
exact 257-byte `.palmask`. Lunar Magic changes only the selected word, preserves all other 256
working colors, retains the complete selector, republishes it beside a subsequent export, and
auto-enables the level's custom palette. An invalid TPL-version import displays the original
rejection, leaves all 257 palette words unchanged, and resets the original's transient selector to
all enabled. Rust deliberately keeps both palette and selector failure-atomic; the stronger state
boundary is recorded explicitly rather than mistaken for an unobserved original behavior.

## 2026-08-08 — Shared and full palette transfer audit

- Lunar Magic executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Pristine headered SMW-US ROM SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`

```text
cargo test -p lm-app --test level_palette_transfer_wine \
  original_lunar_magic_shared_exports_match_both_backends_and_legacy_import_reopens \
  -- --ignored --exact --nocapture
1 passed; 0 failed; finished in 43.77s
```

The isolated Wine gate drives original shared-palette commands `$239D` and `$239E`, opens the
palette editor with `$2528`, and invokes its full-palette export control `$2264`. On the original
legacy backend, both export entry points produce the same `$7E2` bytes with SHA-256
`ea0c7adc6a67abe06d6dee5c57818a8536385b3d1bba7de9e6165466097ea0c2`. Importing a file whose
byte `$123` is changed, then exporting again, reopens that exact file with SHA-256
`7694d2f0dd5fb1535bf9ec82ad021e583239bc5b34fc0a99341fe8d69dcb26e3`; the resulting ROM retains
a valid SNES checksum.

The gate then selects the original process's recovered expanded-palette backend byte and proves
that both original export entry points produce the same `$810` bytes with SHA-256
`3b72d173b2d549fa4a014f8d97e9d7385998b07fe0dfb92daf81f338ab078e08`. Ghidra's recovered
`ImportSharedPaletteFile` supplies the reciprocal expanded read-size and save-path evidence.
Rust independently round-trips both exact backends, installs legacy data into expanded storage,
rejects unsafe downgrade atomically, reopens the installed data, and passes the two built-CLI
process tests. The renderer regression remains green at 232/232 tests.

## 2026-08-08 — Graphics 8×8 editor interaction audit

- Lunar Magic executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Authenticated installed level-105 ROM SHA-256:
  `69cc6693ccd83f67369479314466b53c50e57569d319d9f8078667cfc025928e`

```text
cargo test -p lm-app --test graphics_editor_wine \
  original_graphics_editor_gestures_preserve_private_buffer_and_guard_sheet_paste \
  -- --ignored --exact --nocapture
1 passed; 0 failed; finished in 30.95s
```

The isolated gate opens Lunar Magic 3.63's original `Window8x8`, uses a controlled diagnostic-page
maximum to expose the same internal cache as the retained observations, and then performs real
window flip, foreground/background paint, selection, and right-paste gestures. Its pixel-buffer
output exactly matches `graphics-pixel-buffer/oracle.tsv`: transforms and paint change only the
64-byte private edit tile until an eligible sheet paste. Its sheet output exactly matches
`graphics-cache-paste/oracle.tsv`: `$002` and `$5FF` change to the source, while fixed-animation
`$041`, unused non-bypass `$300`, and first out-of-range `$600` remain unchanged.

The same fresh process invokes the original registered-clipboard entry points. Initial copy emits
exactly 64 zero pixels; publishing four asymmetric `00..0F` rows, pasting, and copying returns all
64 bytes exactly. The complete working ROM remains byte-identical, proving these are transient
editor operations. Twenty-nine native graphics-editor tests and two native clipboard tests cover
the portable, pristine, installed, ownership, revision, worker, cache, and publication variants;
the renderer remains green at 232/232.

## 2026-08-08 — SA-1 first-ExGFX insertion and original-editor reopen

- Authentic SA-1 Pack v1.40 source SHA-256:
  `926d28f2c8b0298b3b1744ac2d90c6e9a64260b7740eab5e195c0cbef38273c3`

```text
LM_SA1_EXGFX_BEFORE=... LM_SA1_EXGFX60_AFTER=... \
LM_SA1_EXGFX_AFTER=... LM_SA1_EXGFX100_AFTER=... \
  cargo test -p lm-app authentic_sa1_first_exgfx -- --ignored --nocapture
3 passed; 0 failed

LM_SA1_PACK_ROM=... WINEDEBUG=-all cargo test -p lm-app \
  --test standard_graphics_install_wine \
  lunar_magic_reexports_rust_sa1_standard_graphics_install -- --ignored --nocapture
1 passed; 0 failed; finished in 18.45s

cargo test -p lm-render
232 passed; 0 failed
```

The first gate compares Rust's complete results against authentic Lunar Magic 3.63 first
`ExGFX60`, `ExGFX80`, and `ExGFX100` transitions; all three are byte-identical across their complete
logical ROMs. This covers each domain marker, the raw reserved owner, fixed helper/table family,
ordinary and expanded-settings pointer storage, RATS payloads, LZ2 streams, ROM-size byte, and
checksum. The live Wine gate starts from the authentic SA-1 Pack, installs and verifies all 52
standard GFX files through Rust, independently inserts each of the three ExGFX files through Rust,
then requires Lunar Magic to export the exact 2,048 source bytes. The `ExGFX100` route also proves
the pointer table follows a first-fit-relocated expanded-settings owner rather than assuming the
canonical `$088000` payload. The renderer regression remains green at 232/232.

## 2026-08-08 — SA-1 mixed-domain first-ExGFX matrix

The same authenticated standard-GFX source was imported by Lunar Magic with each multi-domain
combination. Retained complete-ROM SHA-256 values are:

```text
$60+$80       def974231c41608acd782aebba9854e43c7ca31c964f73abce6a366027fcac09
$60+$100      d2ff9867269b920e903703a9123eaaf4038dbd055595af9c90a5d494618ca5bc
$80+$100      8a8ababe0405963227d8fc98168e439c65b25ea17ad0fbd543460329d8cc7c20
$60+$80+$100  5719e8a7dfc2549dfe188fb5cdadb3b76456af7bf6840342e9f18c4a4e6e0b4c
```

```text
LM_SA1_EXGFX_BEFORE=... LM_SA1_EXGFX60_80_AFTER=... \
LM_SA1_EXGFX60_100_AFTER=... LM_SA1_EXGFX80_100_AFTER=... \
LM_SA1_EXGFX_MIXED_AFTER=... \
  cargo test -p lm-app authentic_sa1_first_mixed_exgfx_domains_are_byte_exact \
  -- --ignored --nocapture
1 passed; 0 failed

LM_SA1_PACK_ROM=... WINEDEBUG=-all cargo test -p lm-app \
  --test standard_graphics_install_wine \
  lunar_magic_reexports_rust_sa1_standard_graphics_install -- --ignored --nocapture
1 passed; 0 failed; finished in 13.74s
```

All four Rust outputs are complete-ROM byte matches. The three-domain gate additionally requires
Lunar Magic to re-export exact `ExGFX60`, `ExGFX80`, and `ExGFX100` source bytes from the Rust ROM.
This recovered the seven distinct first-import marker forms and the domain-dependent allocator
ordering.

## 2026-08-09 — SA-1 subsequent ExGFX directory synchronization

Starting from the retained three-domain result, Lunar Magic imported a second complete directory
in two forms. The first replaces all three files (with a changed `ExGFX80` payload); the second
contains only that changed `ExGFX80`, requiring `ExGFX60` and `ExGFX100` to be deleted. Complete-ROM
SHA-256 values are:

```text
replacement-all  4f935054a5dfbb135ec03810ee1441eee1689613257e8111bae0c39354638fa2
only-ExGFX80     b704be1d424b2b99fe0681e49ccc2028aea935a3aa4de0bbc6ac36e7916417cc
```

```text
LM_SA1_EXGFX_MIXED_AFTER=... \
LM_SA1_EXGFX_MIXED_REPLACE_AFTER=... LM_SA1_EXGFX_ONLY80_AFTER=... \
  cargo test -p lm-app \
  authentic_sa1_directory_sync_reclaims_replaces_and_removes_omitted_files \
  -- --ignored --nocapture
1 passed; 0 failed

LM_SA1_PACK_ROM=... WINEDEBUG=-all cargo test -p lm-app \
  --test standard_graphics_install_wine \
  lunar_magic_reexports_rust_sa1_standard_graphics_install -- --ignored --nocapture
1 passed; 0 failed; finished in 15.27s

cargo test -p lm-app --lib
543 passed; 0 failed; 10 ignored

cargo test -p lm-profile -p lm-render
307 profile tests and 232 renderer tests passed
```

Both Rust synchronization results match Lunar Magic's complete ROM byte-for-byte. The native
directory operation authenticates and reclaims the old owners, replaces every present file, clears
omitted pointers with the correct domain sentinel, retains Lunar Magic's allocation ordering and
checksum-compensation bytes, and commits as one undoable revision. Lunar Magic then reopens the
Rust outputs, re-exports each surviving file exactly, and reports the two omitted files absent.

## 2026-08-09 — Fast-LoROM graphics-compression migration

The authenticated installed LZ2-Orig SMW-US source was changed only to internal map mode `$30`,
checksum-repaired, and presented in headered and headerless physical forms. Lunar Magic 3.63 then
created the LZ3 oracle and exported all 52 standard graphics files. Representative retained hashes
from the capture are:

```text
Fast-LoROM LZ2 headerless  9b27da5162caf891fd5c10ff93dc7954818349694a6513549c9facd7ebc6aca2
Fast-LoROM LZ2 headered    42e04cc5aed6bca1059c4346676133485f6f3a75bd72fb2781e2580d0a8042bf
Lunar Magic LZ3 headered  cf3451fca5a4ad47ea613d7aa6a189ebdf4441398280d44f910e13cb7bd0494d
Rust LZ3 headered          c400be514703c0c453ecd2f2b9018ad8fb18c622b12534e64f59f6de4936cbd2
```

```text
LM_FAST_LZ2_HEADERLESS_ROM=... LM_FAST_LZ2_HEADERED_ROM=... \
LM_FAST_LZ3_ORACLE_ROM=... cargo test -p lm-profile \
  fast_lorom_lz3_migration_matches_across_copier_header_variants \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 9.75s

WINEDEBUG=-all cargo test -p lm-app --test standard_graphics_install_wine \
  lunar_magic_reopens_rust_fast_lorom_lz3_across_copier_header_variants \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 12.72s
```

## 2026-08-09 — ExLoROM graphics-compression migration

Lunar Magic 3.63 converted the retained 8-MiB ExLoROM project from LZ2 Orig to LZ3 while
preserving all 52 standard graphics files and populated `ExGFX80`/`ExGFX81`. Retained physical-ROM
hashes are:

```text
ExLoROM LZ2  26743ed3a747e6bb9e3b60ebcf65103e0b1b7ecb9b53030f924adbd020e27705
ExLoROM LZ3  519eb1a468c067aa821a9e6a47cbd9dfe63fbd787572f5b716724968e1d98ef2
```

The capture establishes that the active compression metadata, hook, ordinary split-pointer
planes, and startup operands are the copies at logical `+$400000`; the base copies remain
unchanged. The installed LZ3 hook points to `$C08008`, resolving through ExLoROM to its owned
runtime payload in the active upper half.

```text
LM_EXLOROM_LZ2_ROM=... LM_EXLOROM_LZ3_ROM=... cargo test -p lm-profile \
  exlorom_codec_replacement_preserves_every_graphics_stream_and_undoes \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 17.93s

LM_EXLOROM_LZ2_ROM=... cargo test -p lm-app \
  exlorom_lz3_command_is_one_same_size_undoable_revision \
  --lib -- --ignored --nocapture
1 passed; 0 failed; finished in 9.33s

WINEDEBUG=-all cargo test -p lm-app --test standard_graphics_install_wine \
  lunar_magic_reexports_rust_standard_and_exgfx_across_legacy_migration \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 61.12s
```

## 2026-08-09 — SA-1 graphics-compression migration

The first SA-1 observation showed that an untouched SA-1 Pack image combines compression changes
with the old-to-4bpp graphics-format upgrade. The isolated codec oracle therefore starts from the
already-expanded 2-MiB 4bpp image containing `ExGFX80`. Lunar Magic's result and Rust's semantic
result retain these hashes:

```text
SA-1 LZ2 source       ea4b793e51aac9f565ea904312d934dea00bb77541a4bea48b83723d7ac8f086
Lunar Magic SA-1 LZ3 bf709d06c410a4dfd761b8f2731dc75880108101a519a88b8a916e212270e303
Rust SA-1 LZ3        a781ee37666295ee6c5366610c906dc9ad69bde27062ed6bee47c1191ac49d33
```

The SA-1 LZ3 hook points into a standalone 780-byte RATS owner with immutable CRC-32 `$520EEB36`
and trailer `LM 01 01`. The source LZ2 hook instead points at addend `$32BA` inside the existing
`$4806` SA-1 owner; its bounded runtime suffix has CRC-32 `$5D9654D6`.

```text
LM_SA1_LZ2_SPEED_ROM=... LM_SA1_LZ3_ROM=... cargo test -p lm-profile \
  sa1_codec_migration_preserves_all_standard_graphics_and_undoes \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 5.21s

LM_SA1_LZ2_SPEED_ROM=... cargo test -p lm-app --test standard_graphics_install_wine \
  lunar_magic_reopens_rust_sa1_lz3_with_standard_and_exgfx_streams \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 6.91s
```

The follow-up mapper-event capture transfers the vanilla overworld into the expanded SA-1 project,
then changes that installed result to LZ3:

```text
SA-1 event LZ2 2a2356dc829a445d7e338bb58623683469f82209babd8328b063fcc73e1b499a
SA-1 event LZ3 61cd097b5c414a823f1dea92ffc1fdc3122fe053303de9dc28d491008b2c95d9

LM_SA1_EVENT_LZ2_ROM=... LM_SA1_EVENT_LZ3_ROM=... cargo test -p lm-profile \
  sa1_installed_event_streams_authenticate_in_both_compression_modes \
  -- --ignored --nocapture
1 passed; 0 failed

LM_SA1_LZ2_SPEED_ROM=... LM_SA1_LZ3_ROM=... cargo test -p lm-profile \
  sa1_codec_migration_preserves_all_standard_graphics_and_undoes \
  -- --ignored --nocapture
1 passed; 0 failed; finished in 5.72s
```

The first gate proves identical Rust logical output across copier forms, map-mode `$30` retention,
valid checksums, exact 52-file semantic equality with Lunar Magic's LZ3 result, and byte-exact Undo.
The end-to-end Wine gate independently creates the original oracle, asks Lunar Magic to select LZ3
again on each Rust output, verifies no logical byte changes, and compares every original re-export
with the source graphics. The original adds its canonical copier prefix to the headerless file, so
that case compares the unchanged logical body while the headered case remains physically identical.

## Historical optimized-LZ2 generation authentication

The retained exact-runtime test authenticates the `$1AF`/`LM 00 01` generation and proves both
payload corruption and a mismatched current-generation trailer reject:

```text
cargo test -p lm-profile historical_lz2_speed_runtime_is_exactly_authenticated
test graphics_compression_runtime::tests::historical_lz2_speed_runtime_is_exactly_authenticated ... ok
test result: ok. 1 passed; 0 failed
```

The non-redistributed patch-derived ROM is supplied explicitly for the corpus gate:

```text
LM_HISTORICAL_LZ2_SPEED_ROM=/tmp/AVSMWFinal.smc cargo test -p lm-profile \
  historical_lz2_speed_rom_migrates_all_graphics_and_events_like_lunar_magic -- --ignored --nocapture
test graphics_compression_runtime::tests::historical_lz2_speed_rom_migrates_all_graphics_and_events_like_lunar_magic ... ok
test result: ok. 1 passed; 0 failed
```

The expanded corpus gate now performs the complete same-size LZ3 migration and compares it with the
retained Lunar Magic result:

```text
LM_HISTORICAL_LZ2_SPEED_ROM=/tmp/AVSMWFinal.smc \
LM_HISTORICAL_LZ3_ORACLE=/tmp/lm-legacy-speed-oracle.wFTkTH/legacy.smc \
LM_HISTORICAL_LZ3_RUST_OUTPUT=/tmp/AVSMWFinal-rust-lz3.smc \
cargo test -p lm-profile \
  historical_lz2_speed_rom_migrates_all_graphics_and_events_like_lunar_magic \
  -- --ignored --nocapture
test ...historical_lz2_speed_rom_migrates_all_graphics_and_events_like_lunar_magic ... ok
test result: ok. 1 passed; 0 failed; finished in 149.65s
```

The gate covers 52 standard files, 54 ExGFX files, both installed event streams, the exact GFX17
upgrade, checksum repair, semantic reopen, and byte-exact Undo. Original Lunar Magic 3.63 then
reported `The ROM is already using this compression format.` for the Rust output. Its SHA-256
remained `67a7c8bd72e4902b3dc28165f952ab0d063ddfd5b54e9656e50a24f2df843563`, and fresh
`-ExportGFX`/`-ExportExGFX` directories matched the original LZ3 oracle's 52/54 files with no
differences. See `oracle-work/graphics-compression-lz2-speed-generation-100.md` for source and
conversion hashes.

The same corpus gate now includes both reciprocal migrations:

```text
LM_HISTORICAL_LZ2_ORIGINAL_ORACLE=/tmp/lm-historical-reverse.Rwc5RB/original.smc \
LM_HISTORICAL_LZ2_SPEED_ORACLE=/tmp/lm-historical-reverse.Rwc5RB/speed.smc \
LM_HISTORICAL_LZ2_ORIGINAL_RUST_OUTPUT=/tmp/AVSMWFinal-rust-lz2-original.smc \
LM_HISTORICAL_LZ2_SPEED_RUST_OUTPUT=/tmp/AVSMWFinal-rust-lz2-speed.smc \
# plus the three forward variables above
cargo test -p lm-profile \
  historical_lz2_speed_rom_migrates_all_graphics_and_events_like_lunar_magic \
  -- --ignored --nocapture
test ...historical_lz2_speed_rom_migrates_all_graphics_and_events_like_lunar_magic ... ok
test result: ok. 1 passed; 0 failed; finished in 483.63s
```

Both reverse results are same-size, checksum-valid, semantically reopen all 52 GFX, 54 ExGFX, and
both event buffers under LZ2, and Undo to the exact LZ3 input. Lunar Magic 3.63 reports `The ROM is
already using this compression format.` for both Rust files, leaves SHA-256 values
`391c4bac9719894f8c63b5c4fc56ea59b576477d55a0eb0a9ffd137973fbd408` (`LZ2 Orig`) and
`58bfd1818513fde7936a4d852c44bf663da9236be2679a0c55684c18d52138f1` (`LZ2 Speed`) unchanged,
and fresh 52-file GFX plus 54-file ExGFX exports match the corresponding original-editor reverse
oracles with no differences.

## Input-driven custom-time gameplay

The supplied driver was rebuilt from the repository, and the official Snes9x libretro core was
built from clean upstream commit `b5cc765` for macOS arm64. The core SHA-256 was
`df2113649ea880ca6b329e4405128c8cab3ff0fb1b8ccbe9fcfe7a20ee09114e`.

```text
tools/build-snes9x-gameplay-driver.sh /tmp/lm-snes9x-gameplay-driver-2
SNES9X_BIN=/tmp/lm-snes9x-libretro.dylib \
SNES9X_GAMEPLAY_DRIVER=/tmp/lm-snes9x-gameplay-driver-2 \
cargo test -p lm-app --test snes9x_smoke \
  rust_custom_time_and_support_patch_b_are_applied_in_snes9x_gameplay \
  -- --ignored --exact --nocapture
test rust_custom_time_and_support_patch_b_are_applied_in_snes9x_gameplay ... ok
test result: ok. 1 passed; 0 failed; finished in 0.95s
```

The gate boots the checksum-valid Rust ROM, advances the title/file/intro sequence, enters the
current level using controller input, and captures a genuine Snes9x state plus gameplay PNG. Rust
requires game mode `$14`, exact custom timer digits `4/5/6` at WRAM `$0F31..$0F33`, bounded image
dimensions, and more than one rendered color. The upstream source, built core, ROM, state, and PNG
remain non-redistributed local evidence.

The ordinary five-byte-header timer was then exercised independently with the same core and
driver:

```text
SNES9X_BIN=/tmp/lm-snes9x-libretro.dylib \
SNES9X_GAMEPLAY_DRIVER=/tmp/lm-snes9x-gameplay-driver-2 \
cargo test -p lm-app --test snes9x_smoke \
  rust_standard_time_music_and_sprite_headers_are_applied_in_snes9x_gameplay \
  -- --ignored --exact --nocapture
test rust_standard_time_music_and_sprite_headers_are_applied_in_snes9x_gameplay ... ok
test result: ok. 1 passed; 0 failed; finished in 0.93s
```

That ROM contains no custom-time command in either candidate level. Both semantically reopen with
ordinary time selector 3, music selector 7, sprite memory `$0B`, both buoyancy controls cleared,
and Layer 1 scroll mode 3.
The captured gameplay state requires exact WRAM digits `4/0/0`, active song `$12` at `$0DDA`,
sprite memory `$0B` at `$1692`, buoyancy flags `$00` at `$190E`, and scroll runtime pair `$00/$00`
at `$1411/$1412`. A separate mode-2 probe produced `$01/$02`, proving the pair is mode-sensitive.
This discriminating tuple
replaces the prior `$03/$12/$C0` observation and proves the input route is executing the edited
level rather than accepting coincidentally matching vanilla values.

The complete original-editor header suite was rerun after the gameplay gates:

```text
WINEDEBUG=-all cargo test -p lm-app --test level_header_wine \
  -- --ignored --nocapture --test-threads=1
test lunar_magic_canonicalizes_every_reserved_mode_without_losing_background_color ... ok
test lunar_magic_exports_every_rust_legacy_level_header_field ... ok
test result: ok. 2 passed; 0 failed; finished in 11.90s
```

This exhausts the original success/canonicalization surface while the Snes9x gates prove the
runtime-sensitive fields and the renderer covers mode, palette, and tileset consequences. The
combined evidence promotes the level-header Oracle gate rather than treating one emulator snapshot
as proof of the whole workflow.

## Input-driven title-movement recording

The supplied driver was rebuilt after adding the bounded `smw-title-recorder` scenario. The same
official Snes9x libretro core identified above booted Rust's vanilla recorder output, whose complete
headerless SHA-256 `663f824b807c8addc81be50b35cd6d2b5f714427063107ddc52aa037c962341f`
is identical to Lunar Magic 3.63's retained expansion result.

```text
tools/build-snes9x-gameplay-driver.sh /tmp/lm-snes9x-gameplay-driver-title
SNES9X_BIN=/tmp/lm-snes9x-libretro.dylib \
SNES9X_GAMEPLAY_DRIVER=/tmp/lm-snes9x-gameplay-driver-title \
cargo test -p lm-app --test snes9x_smoke \
  rust_title_recorder_captures_real_joypad_input_in_snes9x \
  -- --ignored --exact --nocapture
test rust_title_recorder_captures_real_joypad_input_in_snes9x ... ok
test result: ok. 1 passed; 0 failed; finished in 0.61s
```

The real boot traversed game modes `$00..$14`, entered the current level, idled for 600 frames, and
then received B for 12 frames, A for 9, and no input for 7. The runtime published marker `$0042`, a
bounded length, and exact bytes
`00 00 00 00 00 00 00 00 58 80 08 01 80 00 0B 80 C0 01 80 80 08 00 00 07 FF`
at WRAM `$7F:0000`. Rust decoded the genuine serialized state, required a nonblank gameplay PNG,
installed those captured bytes into title playback, and reopened the same semantic recording. The
state, screenshot, ROM, and proprietary core remain non-redistributed local release evidence; the
deterministic expected stream and complete commands are retained here and in the automated gate.

## Map16 bitmap non-aligned edge normalization

A 17×16 top-down 24-bit clipboard BMP (SHA-256
`4d48542d2e9340db0073aad0071654fc40face69a8eaa8115f68e168da00fd10`) was imported by an
isolated Lunar Magic 3.63 process from a vanilla ROM. The odd width forces normalization to a
32×16 working plane and distinguishes partial from wholly synthetic 8×8 cells. Captures with the
configured blank-8×8 option enabled and disabled both pass the complete Rust comparison:

```text
LM_BITMAP_CAPTURE_DIR=/tmp/lm-map16-padding17-8-enabled.0kPwVh \
LM_BITMAP_SOURCE=/tmp/lm-map16-padding17-8.bmp \
cargo test -p lm-app --test map16_bitmap_wine_capture \
  lunar_magic_bitmap_capture_matches_rust_palette_and_graphics -- --ignored --exact
test ... ok

LM_BITMAP_CAPTURE_DIR=/tmp/lm-map16-padding17-8-disabled.YFtqhX \
LM_BITMAP_SOURCE=/tmp/lm-map16-padding17-8.bmp \
cargo test -p lm-app --test map16_bitmap_wine_capture \
  lunar_magic_bitmap_capture_matches_rust_palette_and_graphics -- --ignored --exact
test ... ok
```

The enabled capture's after-state SHA-256 values are
`3fe82bd39a1d0b3e28fded78dc0fdf6d877f116eaad5edd9fd9eedc7654c4eab` (palette),
`d4776ae6b8069d979cc3592c815e09f177658ab534a95a7f17b532ce5f92f21c` (graphics), and
`202857f04d40ca82b0cd5a33d013e8f17331402a731b379126e7101c6bf45c4a` (complete Map16
workspace). The disabled capture uses a different accumulated Map16 baseline, and its complete
after-workspace SHA-256 is
`23bee4f42318c27782ab0925736f7880f80027d67cf0bae9bd3730176b8e7bcd`; the exact gate
reconstructs each result from its own captured baseline rather than comparing unrelated process
histories.

## Map16 bitmap allocation exhaustion

An isolated Lunar Magic 3.63 process imported the retained 32×32 top-down BMP (SHA-256
`7665fbb71678b38e73bfa36ac76428457d95e26f67e0ecdace57b4a97d72752b`) with its first Map16
tile set to `$8FFF`. The source requires four distinct Map16 definitions, but the native allocation
band ends exclusively at `$9000`. Lunar Magic retained the first definition at `$8FFF`, displayed
`Not enough free 16x16 tiles!` and its complete partial-import explanation, and left all later
definitions unchanged. The capture is `/tmp/lm-map16-exhaustion-capture.BtyPpH`.

```text
LM_BITMAP_CAPTURE_DIR=/tmp/lm-map16-exhaustion-capture.BtyPpH \
LM_BITMAP_SOURCE=/tmp/lm-bitmap-matrix.O2tdOm/source.bmp \
cargo test -p lm-app --test map16_bitmap_wine_capture \
  lunar_magic_bitmap_capture_matches_rust_palette_and_graphics -- --ignored --exact
test ... ok
```

The after-state SHA-256 values are
`115b3c585e592008c44f8ab6705dee297e8a648d23f9beb8c0815c06aceceb63` (palette),
`64dc776f697e1e4e8463091af5b1835a0ea9e6a91b7e9f503d101b837eabcdf9` (graphics), and
`7d232c9f53ed488fa1988c3a7238d6634157ea3a0fcfdc442219a9c29808690c` (complete Map16
workspace). Exactly seven Map16 bytes changed, all within definition `$8FFF`. The native session
test `pristine_map16_exhaustion_retains_only_the_successful_preview_prefix` independently proves
the same one-of-four allocation outcome.

## Map16 opaque-black exact-match retention

Three isolated-process captures bind the low-color boundary that was invisible in prior high-color
fixtures. The 16×16 solid-black BMP has SHA-256
`5fdab72d1cb204c38e54c3593ffb0d81ff3708c45fc55b6cbbad4dad9a3fce0b`; the 32×16 black/red
BMP has SHA-256 `37ae916834944a216a5e7665fb9bdf1be0ed07e94b042ca70379101b65cacd11`.
The latter was captured with a one-color limit in both ordinary and Maintain Detail modes.

All three exact Wine comparisons pass. Their palette/graphics/Map16 after-state SHA-256 triples are:

- solid black: `a5ac19bd9cc5ff1ed0be245dd26898b7d6d114873f770d02a1d246889055badd`,
  `ccdfb6d920e3ac76312cb0dfb1ac16e3ad6e72ebc7551d2a52c79021fde22e3d`,
  `a6b80e0e6bf3e69783ff811bfda66d1529db648ba9de8304301fabf72679a738`;
- one-color ordinary: `ea727d06d55680f52fbcc73a3210605b47a4968915d8839e0da3ed010197602a`,
  `ccdfb6d920e3ac76312cb0dfb1ac16e3ad6e72ebc7551d2a52c79021fde22e3d`,
  `34f28ddcff58cd75cf929cfc7ab810975c361094451f3c4c8c8c4677edfffb79`;
- one-color Maintain Detail: `7474781e87653ebcf38b69e64ec130cdcbb460d1ea2e286239ef4d19c3acdc90`,
  `ccdfb6d920e3ac76312cb0dfb1ac16e3ad6e72ebc7551d2a52c79021fde22e3d`,
  `d387c3c658303c5dd961db6497b72236b3c3c691631d1816cd899a4c2d2a5c18`.

Together they prove Lunar Magic retains row 0/index `$D` black without consuming the single
generated-color slot and materializes graphics tile `$200` in both reduction modes.

A fourth 32×16 near-black/red fixture (SHA-256
`a807ec07490de3f7d9755547d6226aed59172eec5d008fec0a10cb14cebf0ccc`) closes the remaining
sentinel ambiguity. With Maintain Detail and a one-color limit, near-black is assigned to the
leading zero-color candidate, but Lunar Magic does not make those pixels transparent: it maps them
to the usable black at row 0/index `$D` and emits a nonblank tile at `$200`. The exact Wine gate
passes with after-state SHA-256 values
`7474781e87653ebcf38b69e64ec130cdcbb460d1ea2e286239ef4d19c3acdc90` (palette),
`ccdfb6d920e3ac76312cb0dfb1ac16e3ad6e72ebc7551d2a52c79021fde22e3d` (graphics), and
`150abbbb9d4045b6467bcf7286888be31419d0c5565b9e1d543bc8ee29053cb4` (complete Map16
workspace). Rust now materializes the temporary zero-color candidate as a nonzero reduced index;
only source alpha produces transparent index zero.

## Overworld ExAnimation runtime installation and mutable submap owner

An isolated Lunar Magic 3.63 session started from the authenticated copier-headered pristine
SMW-US ROM, opened level `$105` and the Overworld Editor, then created one local submap animation
through `Edit Submap Extended Animation Frames (in hex)`: Type index 1, Destination `00A0`, Frames
`00`, source frame `0500`. The helper accepted the explicit `Save overworld to ROM?` prompt.

The saved ROM SHA-256 is
`93e5daddf0229d34232e83f4e40c6d3d7321807dd92644981fe9d1211eb20d5b`. The three contiguous core
RATS owners at logical headers `$08BC66/$08C88E/$08C8AB` have complete SHA-256
`04fb09d57cb18d8d6f6a07cc00c5f15767075a8764182cfb329c8253eb342b26`; the adjacent edited compact
owner at `$08C8BA` has SHA-256
`e6d3ad990be851cbb03cb9d1656eb05bfd0fa16dda71da82163ed3dfc50b980b`.

The capture disproved the prior assumption that all `$15` auxiliary bytes stay immutable: it is a
seven-entry 24-bit pointer table. The authentic first pointer `C2 C8 11` resolves to the adjacent
compact owner, while each empty slot remains exactly `FF 00 00`. Rust now authenticates the full
runtime, fixed writes, and owner chain; the focused six-test runtime suite passes.

## 2026-08-09 — SA-1 permanent level-access restriction

An isolated unmodified Lunar Magic 3.63 process restricted an authenticated copier-headered
SMW-US ROM with SA-1 Pack v1.40 installed. The `$100200`-byte source SHA-256 is
`827f396152867cf296b4e481d916cebd432ae616fee392e488f5828b91cc226d`; the restricted output
SHA-256 is `fd0c016b4ac94849ac1b8b3e546d0c8a80cc87ec23dde3716a8f68357ae78604`.
The retained title is `Codex Parity Test`, keys are `$48/$16/$4DC8`, and the complete output has
29 changed physical ranges while retaining a valid stored checksum of `$D9B0`.

```text
LM_SA1_RESTRICTION_BEFORE=... LM_SA1_RESTRICTION_AFTER=... \
cargo test -p lm-profile \
  level_access_restriction::tests::sa1_restriction_matches_authentic_lunar_magic_output_exactly \
  -- --ignored --exact
1 passed; 0 failed

cargo test -p lm-render
233 passed; 0 failed

cargo test -p lm-native
915 passed; 0 failed; 11 ignored
```

The exact comparison covers every physical byte plus exact Undo/Redo. Ghidra descriptor recovery
proves SA-1 retains the base lower-ROM physical locations, takes no ExLoROM metadata-mirror branch,
and performs a seven-region guarded runtime upgrade during bulk resave. Portable profile, project,
and application tests cover the descriptor, validate-all-before-write atomicity, and mapper route.
Full provenance and recovered boundaries are retained in
`oracle-work/level-access-restriction-sa1-363.md`.

Follow-up port-8089 recovery of `BuildRestoreArchiveFilename` (`004AED30`),
`OpenOrCreateRestoreArchive` (`004AEFB0`), `EnsureRestoreDirectoryExists` (`004AEF00`), and
`EnsureOriginalRomCopyInRestoreFolder` (`004AEE90`) closes the last restriction-workflow variant
gap. The archive is not registry-associated: it is deterministically
`<ROM directory>/sysLMRestore/<ROM stem>.lrp`. Registry values `OrigROMn` are only a later fallback
for finding the pristine original after the project-local `smwOrig.smc`, `smwjOrig.smc`, or
`AllWorldOrig.smc` search. Rust now creates or appends that exact associated archive and reuses the
validated project-local original without opening either chooser.

## Isolated live backend selected-level oracle

On 2026-08-10, the concrete `lm-libretro` backend was built locally and exercised with the
official ARM64 Snes9x libretro core at upstream commit `2ab06b3` and the untouched copier-headered
vanilla ROM SHA-256 `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`.
Neither proprietary input nor the locally built core is redistributed. The repeatable bounded
driver is retained as `tools/lm-libretro-smw-oracle.py`.

```text
cargo build -p lm-libretro
python3 tools/lm-libretro-smw-oracle.py \
  --backend target/debug/lm-libretro \
  --core /tmp/snes9x/libretro/snes9x_libretro.dylib \
  --rom /path/to/vanilla/smwOrig.smc

result  level  frame  mode  translevel  camera  size     frame_sha256                                                     audio
initial 105    1769   14    28          0,192   256x224  d557c220ec3a788e589c4ad9cdf74ef400ab6a2af629f5b9f3ea16d7221f3dc3  32040Hz/534f/9e52af87f6ba41aa481861a96c72d5616ff0ab4e4dc3eef3725e27f46e715412
switch  106    158    14    28          0,192   256x224  be89bf643a51acd6770a4ab012423ed94c7a4e4f3fa8c419ef252a707733e512  32040Hz/533f/510f929fcbaa35d62ac3c724ca3f3acd00ab6a6ee2e3ee1f8e536befc55ffafc
reload  105    1769   14    28          0,192   256x224  d557c220ec3a788e589c4ad9cdf74ef400ab6a2af629f5b9f3ea16d7221f3dc3  32040Hz/534f/9e52af87f6ba41aa481861a96c72d5616ff0ab4e4dc3eef3725e27f46e715412
```

The current run requires capability mask `$1FF`, automatically traverses vanilla game modes
`$00..$0E`,
entered selected sublevel `$105` through `$0F..$14`, switched the active core to `$106` through a
second `$0F..$14` transition, required distinct nonuniform 256×224 opaque RGBA frames, acknowledged
hard pause, required bounded nonuniform interleaved stereo at the declared 32,040-Hz core rate,
reloaded ROM revision 2 in the same process, reproduced level `$105` and its exact frame and audio,
produces exactly one requested paused frame, stops, and exits zero. Before switching levels it now
performs two deterministic `$105` runs: one with the exact original headerless stream and one with
real record IDs/placement changed, while holding identical joypad input. It queries `$14C8`, `$009E`,
and `$1938` every frame and requires the edited Goomba to become active without leaving mode
`$14`/sublevel `$105`, then proves the save-RAM mirror remains bounded/restorable and a direct
switch reaches a distinct `$106` frame. Port-8089 Ghidra
independently proves Lunar Magic 3.63 resolves `LMSW_LoadLevel` and calls it with
`g_dwCurrentLevelNumber` after ROM load and again when the current editor level finishes loading.

### Independent Snes9x 2010 core variant

The same driver passed without relaxation against the official libretro ARM64 buildbot artifact
`snes9x2010_libretro.dylib` dated 2026-08-08, SHA-256
`93159322d61d5721432e1c16c0e9164bc0c0123714d399c6527b50fb1b21d137`. The ROM and video hashes,
transition frame counts, modes, translevel, camera, and geometry exactly match the primary Snes9x
run. The independent core produced 533 stereo frames per observation at 32,040 Hz; the exact audio
hashes were `0532fab7d29d9828a8e0c0d8dd7f6c8e9b460ee7063bbb99f5af13e8d8975fe9` for initial/reloaded
`$105` and `f30ca6804844b2896629fd5f5ab13f15e8c9b86dd77b2e234deb066e0a2674f3` for `$106`.

The current official ARM64 bsnes buildbot core was also probed. It accepts the private full-path
ROM but does not publish SMW WRAM through libretro memory ID 2, even after a bootstrap frame. Rust
therefore rejects initialization with the exact diagnostic `libretro core does not expose exact
128 KiB system RAM after bootstrap; memory maps: none`; it does not claim capabilities it cannot
implement.

### Independent bsnes emulator-family variant

The complete driver also passes against the official ARM64 buildbot
`bsnes2014_accuracy_libretro.dylib` dated 2026-08-08, SHA-256
`591896a857d0cda925a15032856a5026150a5cf5e93a633223f31fb52f2cf9bf`. Unlike Snes9x, this core
publishes valid doubled-width 512×224 frames and 32,041-Hz audio. Exact results are:

```text
result  level  frame  mode  translevel  camera  size     frame_sha256                                                     audio
initial 105    1769   14    28          0,192   512x224  7ac5d5144ab0f15eb1ca294ca9e687fa87f0a30a3cd216f4a32cafa78eb2d601  32041Hz/536f/6f8de3ec1762e08b8a06475178d88a57fb55d6b0d42810027b6c08d2aa293eff
switch  106    158    14    28          0,192   512x224  36d3b9ec7bbc637a25e81f4f5fb8776ba8288e9432ec14d5c7e7ff25953dc1c5  32041Hz/543f/6cf8153d6c17135ab1416f3a7a46d20fa10ba1dd240a125b4364a2fa016033af
reload  105    1769   14    28          0,192   512x224  7ac5d5144ab0f15eb1ca294ca9e687fa87f0a30a3cd216f4a32cafa78eb2d601  32041Hz/536f/6f8de3ec1762e08b8a06475178d88a57fb55d6b0d42810027b6c08d2aa293eff
```

The gate retains every semantic assertion used for Snes9x, including actual edited-Goomba
instantiation without leaving mode `$14`, bounded save-RAM restoration, runtime table queries,
distinct `$106`, and exact `$105` whole-ROM reload reproduction. Supporting both native SNES widths
does not weaken those state or hash requirements.

### Windows x86-64 live-backend runtime

The optimized `x86_64-pc-windows-gnu` build at Git commit
`adccf5074fddd91c8746caabf422798e36ba5b45` produced a PE32+ console executable with SHA-256
`6cf214d5ac377c6eca7fd256495e3755a6ef7dbe5c9f030fe9f8b5565b165f90`. Wine Staging 11.13
executed it against the official 2026-08-08 Windows x86-64 buildbot
`snes9x2010_libretro.dll`, SHA-256
`2ed66e60eb56b302128b9a3cb5831accf47f4e39f4b5a638862cccb339af1b82`, and the identical
copier-headered vanilla ROM.

The complete oracle exited zero. Every result—including transition frame, runtime state, camera,
geometry, frame SHA-256, sample rate/count, and audio SHA-256—exactly matched the native ARM64
Snes9x 2010 table above. It also passed the unprinted in-place edited-Goomba/runtime-table assertions,
pause/step protocol, bounded save-RAM restoration, direct level switching, exact whole-ROM reload,
Stop acknowledgement, and Windows process teardown. The driver invoked Wine as one explicit
`--backend-runner`; no shell was involved.

### Hosted Linux x86-64 live-backend runtime

Git commit `d126b6ca` ran the isolated Ubuntu job
[`93425010747`](https://github.com/YungRaj/lunar-magic/actions/runs/31379108573/job/93425010747)
to completion. Checkout, Rust setup, isolated backend build, strict C11 shared-core build, and the
complete capability exercise all report `success`. The retained deterministic core source has
SHA-256 `b0c53578861a42342b30b190661d5fe767eb6c14c63754dec1205ac7772b996a`; its driver has SHA-256
`003c8ae0ccaf02958f429cd97df28763b0d19777cef822b736845bdf996c1b20`.

The Linux process proves immutable ROM-byte loading, exact 128-KiB WRAM and bounded SRAM discovery,
opaque nonuniform XRGB8888 conversion, interleaved 32,040-Hz audio, direct `$105`/`$106` state,
in-place sprite-stream consumption and runtime-table reporting, distinct level frames, deterministic
whole-ROM reload hashes, pause/step, Stop, unload, and clean process exit. It does not stand in for
SMW emulation: the real vanilla-ROM behavior remains separately proved on native ARM64 macOS and
Windows x86-64, while this job closes the previously missing Linux process/ABI axis.

## Localization auto-detection preference bound

On 2026-08-10, the retained Lunar Magic 3.63 decompilation fixture for
`AutoDetectAndLoadLanguageModule` (`004D7360`) was rechecked against the native selector. The
original compares no more than 64 preferred UI-language tags. Rust now applies that same ceiling
after normalization on every platform, including colon-delimited non-Windows environment sources.
`application::preference_tests::installed_language_autodetection_obeys_original_sixty_four_preference_bound`
proves that an exact match in slot 64 is selected and the same match in slot 65 is ignored; the
neighboring exact-then-primary test continues to prove the recovered two-pass ordering.

## Original language-module checksum and metadata ABI

On 2026-08-10, Ghidra 12.1.2 headlessly reopened the labeled Lunar Magic 3.63 project and
decompiled `ValidateLanguageModuleChecksum` (`004D7010`),
`EnumerateAvailableLanguageModules` (`004D7940`), and
`AppendEnumeratedLanguageMetadata` (`004D77D0`). The resulting contract is retained in the
localization provenance fixture without redistributing an executable or language DLL.

Rust now applies the exact offset-selected rotations/XORs, wrapping negation, third arithmetic
transform, 32-bit accumulation, 64-byte trailer exclusion, and checksum dword location. A four-byte
payload prefix followed by zeroes produces the independently calculated checksum 4,020, then a
single participating-byte mutation is rejected. The metadata gate separately covers the original
marker, optional UTF-8 BOM, CRLF normalization, four recovered fields, and every bounded rejection.
All three `localization::tests::original_language_*` tests pass.

The follow-up portable extractor parses both PE32 and PE32+ without invoking the platform loader or
executing module initialization. Synthetic images retain the exact three-level integer resource
tree (type, resource ID, language), section-relative RVA mapping, and separately placed marker and
metadata payloads. Missing `$DB7`, an unmappable data RVA, and every truncated prefix through the
last required marker byte reject without a panic. All six `original_language_*` tests pass.

The native startup inventory then applies that decoder to regular `.dll` files only in the exact
executable-adjacent `sysLMLanguage` directory. It bounds candidates to 64 and each read to 64 MiB,
skips invalid/unreadable/oversized modules like the original enumeration skips failed loads, and
retains validated filename plus display/version/locale/code-page metadata separately from canonical
`.lmlang` catalogs. Eight configuration-loader tests pass, including deterministic metadata order
and exact candidate-count rejection.

## Original language-module string pool

The same 2026-08-10 headless Ghidra session decompiled `LoadLanguageStringResources`
(`004D6D40`), `LoadSelectedLanguageModule` (`004D7110`), and the raw-DEFLATE streaming helper at
`004A3C20`. Rust now reads `$DAC/$DAD/$DAE` without loading the DLL, exactly reverses the chained
byte transform, requires `StreamEnd`, and applies the recovered
5,869-entry offset/length/NUL validation boundary. Synthetic pools round-trip `hello` and `world`;
a 5,870-entry declaration truncates exactly; one bad terminator clears only its entry; malformed
tables, junk compression, invalid UTF-8, and output past a reduced test limit reject. All three
`original_language_string_resources_*` tests pass.

The same function proves a second validation phase backed by 1,088 bytes at `$005E6420` and 132
bytes at `$005E6398`. Headless byte extraction recovers 272 single-index and 22 range records. Each
record carries an exclusive byte-length ceiling: an in-count string is cleared when its decoded
length is greater than or equal to that ceiling; ranges use an exclusive end. Rust retains the
tables verbatim in `original_language_validation.rs`. Boundary, neighboring-range, exclusive-end,
and isolated-entry regression tests pass as part of the 17-test localization suite.

## Original language-module typed catalog conversion

`CreateMainApplicationMenu` (`00447540`) was re-audited instruction-by-instruction. Every localized
menu insertion reads one dword slot rooted at `$0095BB90`, adds the decoded pool base when nonzero,
and otherwise selects a nearby built-in English pointer before `AppendUtf8MenuItem`. The retained
headless extractor records those index/fallback/function triples. Thirty-one typed equivalents are
now bound, including five top-level menus, ROM open/save/recent/exit, expansion, level analysis,
restore/IPS, Edit clipboard/history, four layer views, emulator actions, Special World Passed, and
Help Contents/About. Rust-only keys remain English rather than borrowing a merely similar original
string.

The five-resource synthetic PE gate validates checksum, metadata, `$DAC/$DAD/$DAE`, fixed-buffer
guards, normalization, typed conversion, and fallback in one call. All 20 localization tests pass.
Native discovery now retains the converted catalog beside original metadata, Language-menu choices
install it directly, and auto-detection compares canonical `.lmlang` and converted `.dll` candidates
in the same exact-then-primary pass. Eight loader tests and 11 preference tests pass.

## Original language-module dialog resource mapping

On 2026-08-10, the labeled project was queried headlessly for `FindLocalizedDialogResourceId`
(`004D76E0`), `ShowLocalizedModalDialog` (`004D7FE0`), and
`CreateLocalizedModelessDialog` (`004D80C0`). The lookup binary-searches 107 original IDs at
`$005E61B8..$005E628D`, maps them through 107 localized IDs at
`$005E62A8..$005E637D`, and accepts a mapping only when the active module contains that type-5
resource. The two 214-byte dumps hash respectively to
`24cb467274b98621cbc92985af83fe0e2e5b918f6d95038f17117e58be3cbdfa` and
`c45a14ded0e8e4c062f828a93784ef1a85a4181eb6a4cddb3d55bf6d73b462da`; rebuilding both
byte streams from the checked-in Rust pairs produced the same hashes.

The portable decoder now authenticates checksum, PE resources, marker, and metadata before exposing
present mapped type-5 templates as borrowed slices. Its synthetic PE fixture contains both the
original `$01F4` resource tree and a separate type-5 tree. Three focused tests prove the exact
107-entry boundary, first/last payload extraction, omission of absent resources, wrong-marker
rejection, and malformed-RVA rejection. This establishes the original fallback ABI but does not
claim native control localization or a live third-party language-DLL gate.

The follow-up portable decoder implements both standard and extended Win32 template framing. Three
synthetic tests cover UTF-16 titles/captions, standard 16-bit and extended 32-bit IDs, ordinal and
named classes, ordinal non-text titles, both font tails, alignment, creation data, every truncated
prefix, invalid UTF-16/version, and non-padding trailers. The local-only executable gate passed all
107 resources independently for `lm363/Lunar Magic.exe` and `lm363/x64/Lunar Magic.exe`.

Decoded Language dialog `$042B` controls 1/2 and About dialog `$03F8` controls 1/`$66`/`$67` are
now semantically bound to five typed Rust actions. A focused catalog test proves mnemonic removal,
Unicode retention, exact key selection, and English retention for unbound keys. This advances real
native catalog output but deliberately does not infer meanings for the remaining decoded controls.

The next retained-catalog gate preserves every decoded literal title/control under an optional
`LMDLG001` extension keyed by original dialog ID, exact template item position, and control ID.
Legacy catalogs re-encode byte-identically; repeated IDs survive round-trip; and exhaustive
truncation plus bad-magic/count/key/duplicate tests reject without partial publication. The
all-five-resource synthetic PE gate now verifies this inventory through the public one-call module
conversion path. Native Modify Secondary Entrances (`$03F1`) consumes its exact title and six
procedure-bound field/action captions with per-control English fallback. The focused localization
suite passed 26 tests (one local-executable gate ignored), and the native binding test passed.

Local executable inventory was then checked for General Options `$041F` and Change Compression
Options `$0416`. Native undo history now binds `$041F` title, `$66`, and IDs 1/2. Native graphics
migration binds `$0416` title, `$65`, `$294`, `$296`, `$69`, and IDs 1/2. Focused tests inject
partial translated inventories and prove exact-ID selection plus independent typed/English fallback.

Optimized LZ2 `$295` was then added to the same native compression selector. A focused route test
proves it emits `InstallLz2SpeedRuntime` at the exact open project revision without consulting the
unrelated allocation text fields. Installed-mode detection chooses Speed as the next action from
LZ2 Orig and LZ3 from installed Speed/LZ3.

The Tools menu now opens an in-app external-tool/emulator editor rather than requiring every user
to author `LMTOOLS1` bytes externally. Setup Emulator `$0407` binds the visible title, executable
path `$66`, arguments `$68`, and IDs 1/2. A focused round-trip test preserves argv boundaries,
working-directory template, and all three event subscriptions; a partial translated inventory test
proves exact control selection and fallback. Existing `state::tests::tools` gates retain atomic
duplicate-ID/event rejection for the final `AppState::set_external_tools` publication boundary.

A second creation action now builds a persisted GBA emulator profile. Its stable `gba-` ID prefix
round-trips through `ExternalTool`, causes the reopened draft to select original dialog `$0408`, and
uses that dialog's exact title/control inventory. A focused test proves `$0408` selection, default
`{rom}` argv, stable generated identity, and fallback title without changing `LMTOOLS1` framing.

The `$0407/$0408` control `$67` short-ROM-path option now round-trips as `{rom_8dot3}` inside an
ordinary argument template. Seven core expansion tests and four editor tests pass. Both
`lm-windows` and `lm-app` cross-compile for `x86_64-pc-windows-gnu`, binding the safe bounded
`GetShortPathNameW` route; the portable gate proves non-Windows invocation fails explicitly while
still recognizing the placeholder for configuration inspection.

The authenticated 3.63 CHM's button/global option tables now bind
`LM_ALLOW_MULT_INSTANCES`, `LM_ALLOW_MULT_INSTANCES_FORCE_ALL`, and `LM_NO_CONSOLE_WINDOW` to the
native user-toolbar launcher. Focused policy tests prove per-button/global selection and default
same-button de-duplication; a real two-child Unix process test proves concurrent ownership and
cancellation. `cargo check -p lm-native --target x86_64-pc-windows-gnu` proves the conditional
`CREATE_NO_WINDOW` launch path compiles.

The same authenticated option table now binds `LM_OPEN_OTHER`. Rust routes it through the existing
approval gate and a platform association opener, then intentionally retains no child ownership, as
the original documentation requires. Focused tests bind option selection, macOS target/argument
boundaries, and Windows quoting of empty, spaced, quoted, and trailing-backslash parameters; both
`lm-windows` and `lm-native` cross-compile the `ShellExecuteW` path for
`x86_64-pc-windows-gnu`.

The original CHM and Ghidra callback inventory now bind the `$BECA` process-notification wire.
`lunar_magic_notification_wire_format_binds_every_documented_type_and_boundary` covers all seven
types, `$6942`, and the exact ten-bit limit. Native workers publish real child PIDs; the Windows
safe boundary retains the ROM-path caption HWND, enumerates PID-owned top-level windows without a
visibility check, and posts the packed payload. Exact option-selection tests cover per-button and
force-all new-ROM/new-level/close routes, and the complete native Windows target cross-compiles.
Save-level, save-Map16, and save-overworld domain marks now publish only after successful physical
ROM persistence; focused success/failure tests bind acknowledgement and suppression, and a domain
coalescing test proves each type is consumed once. Delete-level publication remains unclaimed with
the then-missing native deletion operation.

The authenticated 3.63 CHM defines single-level deletion as replacing an expanded-area level with
the original-area “test” level. A live documented command-line run against the retained modified
level-000 fixture (`-DeleteLevels ... -LevelList 0`) reported `Deleted 1 level.` and changed the
Layer 1 pointer from `10:8008` to `06:8000` and the sprite pointer from `1C:9ED5` to `1C:E76D`, while
reclaiming displaced tagged storage and repairing the ROM checksum. The new project-layer primitive
binds the safe core of that behavior with focused redirect, shared-reference, reclamation, checksum,
and byte-exact undo tests. Aggregate per-level assets and the UI/type-6 route remain open.

The follow-up aggregate oracle executes the same authenticated command under Wine and compares the
Rust mutation against Lunar Magic for Layer 1, sprites, Layer 2, palette, ExAnimation, descriptor,
expanded-settings, entrance, and Lfix3 target records. Every modeled record matches. Both tools
zero the four displaced payload regions; Lunar Magic additionally writes two unreferenced
`STAR FD 01 02 FE` zero reservations for its allocator bookkeeping. Application tests prove one
revision, stale-command rejection, complete-byte Undo, confirmation gating, and the exact type-6
subscription option.

The extracted authenticated 3.63 CHM `info_LM_options_button.htm` closes the default duplicate-tool
gesture: without `LM_ALLOW_MULT_INSTANCES`, clicking the same button again switches focus to the
already open program. The original executable imports `EnumWindows`, `GetWindowThreadProcessId`,
`IsIconic`, `ShowWindow`, and `SetForegroundWindow`, matching the process-owned top-level-window
boundary. Rust now refreshes the tracked child PID, finds the first visible owned window, restores
it when minimized, and requests foreground activation without creating a second process. Focused
launcher tests retain pending/running de-duplication and the Windows target cross-compiles.

## Lunar Magic 3.63 Recent Files popup (2026-08-10)

Port-8089 Ghidra recovery binds `LM_FILE_RECENT_MENU` `$23DB` to central dispatcher case `$3D`.
The case refreshes state, builds the temporary recent menu with `FUN_004790A0`, tracks it at the
pointer, and destroys it after dismissal. The builder proves the ten-entry bound, disabled empty
item, populated separator, and `$23DA` clear command. Executable strings bind the empty label,
clear label, confirmation title, and confirmation sentence. Rust now exposes that popup through
the original user-toolbar route, delegates path selection to the existing recent-ROM lifecycle,
and persists clearing only after confirmation. Focused route, interaction, and partition tests
cover the publication boundary; provenance is retained under
`docs/oracle-work/lm363/recent-menu/`.

## Lunar Magic 3.63 deprecated Options entries (2026-08-10)

The command-byte table at `command_id + $004965D3` was re-read from the live port-8089 program.
`LM_OPTIONS_CUSTOM_SPRTES` `$24C5`, `LM_OPTIONS_WHEEL_ZOOM` `$24DC`, and
`LM_OPTIONS_ZOOM_MENU` `$24DD` each contain `$DF`; `HandleLevelEditorCommand` ends at case `$DE`.
Rust therefore classifies all three as explicit successful no-ops instead of assigning behavior
from their obsolete labels. Focused stable-ROM/state tests, the complete 317-command partition,
and the Windows cross-build pass. Provenance is retained under
`docs/oracle-work/lm363/deprecated-options-no-ops/`.

## Lunar Magic 3.63 Auto-Deselect on Editor Select (2026-08-10)

The live command table maps `$24DB` to case `$A4`, and the retained 3.63 CHM binds the effect to
new Add Object/Sprite or Map16 editor selections—not ordinary main-canvas clicks. Rust now persists
that independent application preference and applies it to all native object/sprite selector
families plus Map16 tile/rectangle selection. Focused tests prove disabled preservation, enabled
cross-domain clearing, toggle status, persistence/reopen, and exact command classification.
Provenance is retained under `docs/oracle-work/lm363/auto-deselect-on-editor-select/`.

## Lunar Magic 3.63 ROM user-area scan (2026-08-10)

An isolated Wine prefix opened a headered 2-MiB SMW-US ROM attributed to Lunar Magic 3.63 and
invoked File → Scan ROM. The unmodified expanded image reported protected `$8C95`, free
`$17736B`, total `$180000`, five structures, largest bank `$8000`, and largest area `$10E1A3`.

A controlled copy added an outer RATS full range at logical `$100000..$100030` and a nested range
at `$100010..$100020`. The dialog reported one conflict and `$10` conflicted bytes. Its generated
`RATS.log` used physical addresses `$100200` and `$100210`, complete sizes `$30` and `$10`, and
overlap `$10`, proving that the copier prefix participates in log addresses. The retained values,
exact original log line, mutation description, and static function/resource provenance are under
`docs/oracle-work/lm363/pristine-us/rom-user-area-scan/`; no ROM or executable is retained.

## Lunar Magic 3.63 per-ROM VRAM patch options (2026-08-11)

The live dialog reached through `LM_OPTIONS_VRAM` `$24E8` has title `Change VRAM Patch Options`
and radio controls None `$294`, Normal `$295`, HD 16:9 `$296`, and HD 21:9 `$297`. On a pristine
ROM None and Normal are enabled with Normal selected by default. After the ordinary runtime is
installed, None is disabled and Normal remains selected; both HD choices are disabled in this
ordinary-LoROM observation. The extracted CHM states that changes take effect on the next level
save and that an unrecognized installed patch disables all choices.

Ghidra `CheckInstalledVramPatchCompatibility` authenticates the primary JML owner through its RATS
payload trailer `LM` plus generation `$0115`, accepts `$0114` for replacement, and rejects unknown
or future generations. `InstallVramPatchRuntime` installs PE resource `$1FD`, executes its
`LMRELOC1` records, writes the complete fixed hook table, and updates the version field. The
retained first-save oracle owns payload `$080962..$083CF2`; its exact relocated payload and all
fixed writes are covered by Rust tests. Hashes, resource geometry, function addresses, and the
clean-room evidence boundary are recorded in
`docs/oracle-work/lm363/vram-patch-options/PROVENANCE.md`.

## Renderer audit offset and joined-GFX route verification (2026-08-11)

The renderer crate passed 234/234 tests and
`every_pristine_level_materializes_its_builtin_render_assets` passed across all 512 vanilla slots.
The audit harness defect was isolated by identical level `$105` editor hashes at requested offsets
`$0` and `$8`; after adding orientation-aware major-axis scrolling, the same captures produced
distinct hashes `8fd03ca5a9360ba9ed2aa95d2dabe10d5b42f382d299e20e24135c2594caaa70`
and `102e18d35723e74b5f2d652d6802f711daba4b583be759c9107e8e5bc7c32611`.
Independent game-preview offsets `$0` and `$8` were also distinct. The 38-test authenticated
user-toolbar suite then passed with `LM_OPTIONS_ATTACH_FILES` routed to the existing persisted
joined-GFX mode and the complete partition at 288 routed / 29 pending.

The adjacent `LM_OPTIONS_AUTO_SCREENS` `$24BC` route was then recovered as dispatcher case `$87`.
The live byte `$005E76F9` was one at startup; Ghidra's registry serializer does not load or save it,
matching a session-only default. A focused end-to-end test expands pristine level `$105`, stages a
deliberately different Last Screen value, saves once with the option enabled and once disabled, and
semantically reopens the ROM. Enabled mode matches the highest visible object/sprite screen;
disabled mode retains the manual five-bit header. Both focused auto-screen tests and all 39
authenticated toolbar tests pass; the partition is 289 routed / 28 pending.

## Context-sensitive custom collection append (2026-08-11)

Ghidra command-table byte `$D3` maps both `LM_KEY_ADD_CSPRITE` and `LM_KEY_ADD_CUSTOM` at `$26AF`.
`HandleLevelEditorCommand` branches on the active sprite/object mode, requires a nonempty current
selection, prompts for a description, and calls `AppendAndReloadCustomObjectTemplate` at
`$0052CB70` or `AppendAndRefreshCustomSpritePlacement` at `$00576E00`. Their core append routines
use ROM same-stem `.mw0/.mw0t` and `.mw2/.mwt` pairs, 32-KiB per-buffer bounds, `(not specified)`
for an empty description, and exact success/failure status resources. The Rust route preserves the
authenticated file and prompt boundary while strengthening the original sequential writes into one
paired atomic publication. Focused create/append/reopen, boundary-marker, malformed/incomplete,
cancel, selection-coordinate, alias, and exact-status tests pass. The renderer remains green at
235/235 and the full native gate materialized all 512 pristine levels without a render failure;
the only first-pass failures were the intentionally advanced command partition counters, updated
from 300/17 to 302/15.

## Responsive viewport crop regression (2026-08-11)

A live pristine level `$105` capture exposed that responsive cover scaling could crop the opposite
edges of the nominal 256×224 camera frame. The native editor now contains and centers that complete
frame with square pixels, using surplus pane space to reveal adjacent level content. Focused aspect,
zoom, horizontal-resize, windowed, full-screen, and live-frame geometry tests pass. A fresh visual
capture was inspected; the renderer passes 235/235 tests, the native suite passes 1,121 tests with
13 external-fixture ignores, all 512 pristine levels materialize, the Windows i686 check passes,
and the regenerated semantic manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`.
