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

The Rust canvas uses its typed insertion-at-position transactions for the supported single active
selection. An unmodified secondary click duplicates and relocates the selected Layer 1 object,
object-backed Layer 2 object, or sprite; the inserted record becomes selected and the source remains
present. Modified secondary clicks remain unconsumed for higher-level gestures.

`right_click_duplication_repositions_objects_and_sprites_without_removing_sources` verifies the
source-preserving clone, exact target cell, selected inserted record, and semantic sprite fields.
Multi-selection displacement remains a separate parity boundary.
