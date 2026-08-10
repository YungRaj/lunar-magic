# Lunar Magic 3.63 screen-exit command recovery

Authenticated target: the labeled `Lunar Magic.exe` program in the live Ghidra project served at
`127.0.0.1:8089` on 2026-08-10.

`HandleLevelEditorCommand` at `00492B80` dispatches command `$2523` to the 32-entry screen-exit
dialog. It first calls `BuildPackedScreenExitArrayFromObjects` (`0043ACD0`) when needed, snapshots
all 32 packed values, and opens the localized modal dialog. Cancel restores the entire snapshot.
OK compares every slot, calls `SetScreenExitObjectForScreen` (`0043AD90`) for changed screens,
refreshes the editor, and captures one `$80000000` level Undo snapshot.

The recovered setter distinguishes absence with packed bit `$10000`; absent entries call
`DeleteScreenExitObjectForScreen` (`0043AD30`). Present entries reuse or create a screen-exit
object, preserve the unrelated advance-screen bit on retained records, choose command parameter
`$00` or `$02` from the destination high nibble, and insert new records at the normalized list-end
position. `DeduplicateScreenExitObjectsByScreen` (`00437190`) traverses backward so the last source
record wins, removes duplicates, and reinserts the retained records in ascending screen order.

The adjacent mouse commands are intentionally not treated as aliases. `$26FE` obtains the current
client mouse cell and calls `FollowScreenExitDestinationAtCell` (`00489CD0`); `$26FF` obtains the
cell and calls `OpenScreenExitEditorAtCell` (`00489C80`). Rust now retains the actual painted level
canvas geometry, resolves the live pointer through zoom, bounds, and horizontal/vertical axis
orientation, and routes `$26FF` to the same complete table with the targeted screen preselected and
scrolled into view. Outside-canvas and out-of-level cells remain no-ops. `$26FE` now uses that same
cell mapping, resolves direct and midway destinations plus the full 8,192-entry secondary table,
rejects absent and overworld exits with the original status text, and navigates clean levels. A
modified level instead enters Save/Discard/Cancel: Discard abandons only the staged controller and
navigates, Cancel retains it, and Save waits for the exact revision-bound commit before navigating.
If Layer 1/2 relocation first needs pristine-ROM expansion, navigation waits through both successful
revisions; any failed or stale expansion/commit clears the deferred destination.

Rust binds `$2523` to a complete 32-screen staged form. `ObjectEdit::ReplaceScreenExitTable`
performs the recovered deduplication, absence, retention, encoding-shape, and ordering behavior as
one atomic controller edit, so Apply contributes one staged Undo step and Reset discards only the
unapplied form. Focused core, editor-form, and authenticated-toolbar partition tests bind this
route.
