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
The no-neighborhood capture now matches exactly after recovering weighted RGB555 source mapping,
row-zero sentinel tie behavior, aggregate subset weights, and first-installed HSL run ordering.
Method 1 and method 2 still reject with different selected palettes and encoded graphics. They
remain proven parity defects and are not counted as passing Oracle or Variants gates.
