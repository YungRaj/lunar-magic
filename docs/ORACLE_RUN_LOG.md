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
1 passed; 0 failed; finished in 46.35s
```

The gate imported records at both valid table endpoints (`$0000` and `$1FFF`) with minimum and
maximum packed fields, required exact Lunar Magic re-export, and independently reopened both
installed ROM-table entries through Rust. It then imported an empty secondary-exit set, required
Lunar Magic to export an empty set, reopened both endpoint entries as native zero records, and
verified the final ROM checksum.
