# Lunar Magic 3.63 Layer 3 bypass command recovery

The authenticated command table maps `$2533/$2534` to `LM_LEVEL_LAYER3_BYPASS` and
`LM_LEVEL_LAYER3_BYPASS2`. Their jump-table bytes are `$BD/$BE` in
`HandleLevelEditorCommand` (`00492B80`).

Disassembly resolves the decompiler's hidden `thiscall` selector precisely:

- case `$BD` calls `ManageSuperExGfxConfiguration` (`0048E900`) with selector 2 when
  `DAT_005E7ADF` is set and selector 3 otherwise (`00496AC6/00496AE4`);
- case `$BE` calls the same function with selector 4 when set and selector 5 otherwise
  (`00496B0B/00496B29`).

`ManageSuperExGfxConfiguration` opens the ROM, verifies the Super ExGFX/expanded-settings
prerequisite, installs the prerequisite when absent, loads standard and extended graphics pointer
tables, presents the selected Layer 3 GFX/tilemap bypass dialog, saves accepted settings, refreshes
graphics resources, and marks the level modified.

Rust routes both variants to the current-level expanded-settings editor. It detects the installed
owner through the authenticated runtime operand rather than assuming the preferred allocation. On
a pristine ROM it selects the complete Layer 3 installer; only a successful revision-bound commit
sets the one-shot continuation that re-detects the relocated owner and opens the editor. Cancel,
failure, or an unrelated later installation cannot trigger the continuation. The editor exposes
custom Layer 3 tilemap file/length/destination/mode fields and Super GFX bypass assignments, with
staged commit, checksum, stale-revision, and dirty-close protection.

`layer3_bypass_route_installs_then_reopens_detected_settings` proves the pristine install, detected
reopen, and byte-exact Undo path. The command-partition test binds both original IDs.
