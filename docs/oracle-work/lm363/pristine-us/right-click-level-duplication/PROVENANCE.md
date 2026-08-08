# Lunar Magic 3.63 right-click level duplication

The live Ghidra service for the authenticated Lunar Magic 3.63 executable was queried on
2026-08-07. The following labeled functions establish the original interaction boundary:

- `HandleObjectModeRightButtonDown` at `0048AB00` routes an unmodified right-button gesture to
  `DuplicateSelectionOrCreateObjectAtPosition`, reports placement failures, and begins a move drag
  after a successful clone.
- `DuplicateSelectionOrCreateObjectAtPosition` at `00438FB0` sorts and clones the selected object
  nodes, snaps their displacement to the requested tile, reinserts the clones, preserves the
  sources, and returns success code 2.
- `HandleSpriteModeRightButtonDown` at `0048AE80` provides the corresponding sprite-mode route.
- `DuplicateSelectedSpritesAtCell` at `004CE840` clones the selected sprite nodes, bounds and
  applies the cell displacement, preserves the sources, and returns success code 2.
- `ValidateAndSnapSelectedObjectDrag` at `00438DC0` converts the proposed pixel motion to one
  whole-tile delta, checks that delta against every selected object, and searches for a shared
  nearest valid displacement when any member would leave the `$3800`-cell editor cache.
- `ReinsertMovedLevelObjectsByTileDelta` at `00438540` applies the shared delta, rebuilds each
  encoded screen/cell position, reinserts in screen order, and regenerates the rendered cache.
- `ClampSelectedSpriteGroupMove` at `004CE610` provides the corresponding all-members-valid
  boundary for selected sprites.

The Rust canvas uses its typed insertion-at-position transactions for the supported single active
selection. An unmodified secondary click duplicates and relocates the selected Layer 1 object,
object-backed Layer 2 object, or sprite; the inserted record becomes selected and the source remains
present. Duplication occurs on the physical press, enters a secondary-button move drag immediately,
and applies the final bounded position on release, including releases outside the canvas. Modified
secondary clicks remain unconsumed for higher-level gestures.

`right_click_duplication_repositions_objects_and_sprites_without_removing_sources` verifies the
source-preserving clone, exact target cell, selected inserted record, immediate drag state, release
cleanup, and semantic sprite fields. Multi-selection displacement remains a separate parity
boundary.

The orientation-neutral `ObjectStream::duplicate_ordinary_object_group` foundation now clones an
ordered selection with one shared major/minor tile delta, retains every source and extension byte,
canonically rebuilds screen transitions once, returns the clone indexes in selection order, and
rejects empty, duplicate, non-positioned, opaque-control, or any-member-out-of-bounds input without
changing the stream. `group_duplication_preserves_sources_relative_delta_and_selection_order` and
`invalid_group_duplication_is_failure_atomic` cover that model/transaction boundary. Canvas
multi-selection and the original nearest-valid fallback remain the next interaction layer.
