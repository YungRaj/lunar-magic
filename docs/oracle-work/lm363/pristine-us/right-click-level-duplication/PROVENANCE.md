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

The same authenticated command dispatcher establishes the aggregate keyboard boundary. Its recovered
command-index table maps `$245D` to `SelectAllLevelObjectsInLayer` (`00436E70`), `$245B` to the
object/sprite delete branch containing `DeleteSelectedLevelObjects` (`00439260`), and `$245C` to the
corresponding duplicate branch. Select All walks the active Layer 1 or object-backed Layer 2 list and
excludes command-zero control nodes. The ignored
`lunar_magic_select_all_deletes_every_positioned_object_and_preserves_controls` Wine gate executes
the actual `$245D` then `$245B` commands in Lunar Magic 3.63 on pristine level `$105`, saves, and
compares Lunar Magic's own before/after MWL exports. Every positioned object is removed, opaque
controls and every non-object domain remain exact, redundant screen jumps are canonically dropped,
and the reopened ROM checksum is valid. The native canvas now consumes Ctrl+A in its focused active
domain and applies Select All, Insert duplicate, and Delete to complete object, Layer 2 object, or
sprite groups atomically.

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
`right_drag_duplicates_and_moves_a_complete_object_group_atomically` cover the native object
selection, clone, immediate-drag, release, and source-preservation workflow.
`object_select_all_shortcut_excludes_controls_and_group_delete_is_atomic` and
`object_group_shortcuts_duplicate_and_delete_the_complete_selection` bind the recovered aggregate
command semantics to the native active-domain shortcut workflow.

The matching sprite operation decodes legacy or expanded records once, tracks selected identities
independently of control-token and canonical sort changes, and rebuilds the minimum upper-Y control
stream once after validating every translated member. Custom record extension bytes remain exact.
Ctrl selection, primary group drag, right-press clone plus immediate group drag, Insert duplication,
and Delete removal all share the atomic aggregate transaction.
`legacy_sprite_group_clone_and_move_track_every_reordered_record`,
`expanded_sprite_group_rebuilds_controls_and_preserves_extensions`,
`sprite_group_transactions_commit_once_track_order_and_undo_atomically`,
`right_drag_duplicates_and_moves_a_complete_sprite_group_atomically`, and
`sprite_group_shortcuts_duplicate_and_delete_the_complete_selection` cover the model, application,
and native workflow.

`FindNearestValidTileMoveDelta` at `00438C50` and `FindNearestValidSpriteMoveOffset` at `004CE430`
recover the final shared-displacement correction order. When one selected reference cell rejects the
requested delta, Lunar Magic searches the major delta from its request toward zero for the current
minor delta, then walks the minor delta toward zero and restarts the major search. Zero is rejected
only as a fallback candidate. After a correction, complete selection validation restarts from the
first member; failure to find a nonzero candidate restores the drag origin. The native object and
sprite group clone/move paths now share that exact search and restart contract over the active
orientation's 512-by-27 or 512-by-32 editor bounds.
`nearest_valid_group_delta_matches_lunar_magic_search_and_restart_order` covers valid passthrough,
zero passthrough, nested search order, complete-selection restart, and no-candidate behavior;
`sprite_group_drag_falls_back_to_the_nearest_shared_in_bounds_delta` binds the correction to the
aggregate native transaction and selected-index tracking.
