# Lunar Magic 3.63 overworld custom-sprite gesture capture

This fixture was captured on 2026-08-09 from the authenticated Lunar Magic 3.63 executable under
Wine Staging 11.13. The input began as the repository's pristine SMW-US fixture. A disposable Rust
project expanded only the temporary copy to 1 MiB and installed two four-byte native custom
overworld records on map 0 through the descriptor-selected stream pointer at logical `$07755D`.
The stream was owned by a valid `STAR` allocation at logical `$080000`; no repository ROM was
modified.

The original editor was opened with command `$232D`, then switched to Sprite Editor Mode with
command `$1FA8`. The canvas was the original `WindowOverworld` child with a 512×432 client area.
The two injected records were centered at canvas positions `(264,264)` and `(296,264)`.

- `selected-two.png` follows a plain click on the first record and a Ctrl-held click on the second.
- `alt-property-dialog.png` follows Alt-right-click on the painter-hit first record. Lunar Magic
  opens the modal `Modify Custom Sprite Manual (in hex)` dialog, populated with command `2`, height
  `0`, and extra byte `00`; cancelling returned to the unchanged canvas.
- `right-drag-group.png` follows selection of both records and a right drag from `(264,264)` to
  `(264,304)`. Both originals remain and two copies appear together at the common snapped offset;
  the canvas status reports `Move sprites 0x5`.

The first exact-sized-owner run also reached the original `Not enough room...!` save rejection after
the successful transient duplicate. That capacity-specific save result is not used to claim gesture
failure: Ghidra's `DuplicateCustomOverworldSpritesAtPosition` (`0055C140`) mutates the working list,
while `CommitCustomOverworldSpriteRecords` (`004BE670`) independently enforces backing capacity.
The retained captures bind only the interaction behavior, which is the matrix gap they close.

SHA-256:

- `selected-two.png`: `03e534d27b83d07478eb4dc3e663884318d6e1476f0c265f91d91c54f53e27cc`
- `alt-property-dialog.png`: `68c267ae6b6fd3bb42182038e74b4aaa0f509d87ca5c32543f93bcb2b73838de`
- `right-drag-group.png`: `b2d40f58ded2df2cae8c2bd70f14addc5c706346a6485455e41981d11ce8b969`

All three captures are 1202×1252 RGBA PNGs. They retain the complete original overworld frame so
the active mode, source placements, modal ownership, and source-preserving duplicate result remain
auditable together.
