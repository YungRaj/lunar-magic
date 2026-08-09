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
candidate count is 17: the requested 16 opaque colors plus a leading zero sentinel. That sentinel
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

The first gate proves identical Rust logical output across copier forms, map-mode `$30` retention,
valid checksums, exact 52-file semantic equality with Lunar Magic's LZ3 result, and byte-exact Undo.
The end-to-end Wine gate independently creates the original oracle, asks Lunar Magic to select LZ3
again on each Rust output, verifies no logical byte changes, and compares every original re-export
with the source graphics. The original adds its canonical copier prefix to the headerless file, so
that case compares the unchanged logical body while the headered case remains physically identical.
