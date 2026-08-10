# Lunar Magic 3.63 Super GFX Bypass command recovery

The authenticated table maps `$251E/$251F` to `LM_LEVEL_SUPER_BYPASS` and
`LM_LEVEL_SUPER_BYPASS2`. The command-byte map at `00498960` assigns cases `$A8/$A9`; the pointer
table at `004985E0` maps those to `00496AB7/00496ADE`.

Disassembly proves case `$A8` calls `ManageSuperExGfxConfiguration` (`0048E900`) with selector 2
when `DAT_005E7ADF` is set and selector 3 otherwise (`00496AC6/00496AE4`). Case `$A9` enters the
selector-3 path directly at `00496ADE`. These are distinct from the old FG/BG and SP list commands
`$2520/$2521`, whose cases `$AA/$AB` use selectors 1/0 and remain separately inventoried.

Both Super GFX commands share Lunar Magic's prerequisite behavior with the Layer 3 bypass family:
they install the Super ExGFX/expanded-settings runtime when absent, load the graphics pointer
tables, edit per-level assignments, refresh graphics, and mark the level modified. Rust therefore
routes them to the same detected current-level settings workspace and complete Layer 3 installer
continuation. The editor exposes all six FG/BG and four sprite assignments plus the enable flag;
commit, reopen, checksum, stale revision, dirty close, and byte-exact Undo are already covered.

The pristine install-and-reopen test covers their shared prerequisite path. The authenticated
command-partition test binds both additional names and IDs.
