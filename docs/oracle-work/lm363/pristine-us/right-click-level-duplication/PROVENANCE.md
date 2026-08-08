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

The Rust canvas uses Ctrl-modified physical left presses to add or remove Layer 1 or object-backed
Layer 2 objects from one domain-exclusive selection set. An unmodified press on a selected group
member preserves the set for dragging. An unmodified secondary press duplicates the complete object
selection with one anchor-relative tile displacement, selects only the clones, and begins a shared
move drag immediately; release applies one atomic delta to every clone. Single object, Layer 2
object, and sprite selections retain their established clone-and-drag route. A failed member bounds
check publishes neither a partial move nor a partial clone, and releasing outside the canvas clears
the transient drag without moving the group. Modified secondary clicks remain unconsumed for
higher-level gestures.

`right_click_duplication_repositions_objects_and_sprites_without_removing_sources` verifies the
single-selection source-preserving clone, exact target cell, selected inserted record, immediate
drag state, release cleanup, and semantic sprite fields.

The orientation-neutral `ObjectStream::duplicate_ordinary_object_group` and
`relocate_ordinary_object_group` operations clone or move an ordered selection with one shared
major/minor tile delta, retain every source and extension byte when cloning, canonically rebuild
screen transitions once, return selection indexes in caller order, and reject empty, duplicate,
non-positioned, opaque-control, or any-member-out-of-bounds input without changing the stream.
`group_duplication_preserves_sources_relative_delta_and_selection_order`,
`invalid_group_duplication_is_failure_atomic`,
`group_relocation_moves_every_member_once_and_tracks_reordered_indexes`, and
`invalid_group_relocation_is_failure_atomic` cover that model/transaction boundary.
`ctrl_object_selection_toggles_members_and_keeps_layer_domains_exclusive` and
`right_drag_duplicates_and_moves_a_complete_object_group_atomically` cover the native selection,
clone, immediate-drag, release, and source-preservation workflow. Sprite multi-selection and the
original nearest-valid fallback remain separate interaction boundaries.
