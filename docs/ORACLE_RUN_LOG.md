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
