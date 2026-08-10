# Lunar Magic 3.63 undefined-exit scan recovery

The authenticated internal-command table maps `$2526` to `LM_LEVEL_SCAN_EXITS`. Ghidra's central
`HandleLevelEditorCommand` dispatcher (`00492B80`) maps the corresponding jump-table byte `$B0`
to `ReportInvalidExitObjectDestinations(owner, 0)`.

`ReportInvalidExitObjectDestinations` calls `FindScreensWithInvalidExitObjects`, formats affected
screens as two-digit hexadecimal values, and warns that an exit-enabled pipe or door targeting
level `$000` or `$100` can trap the player in an endless bonus game. A clean manual scan reports
success instead.

`FindScreensWithInvalidExitObjects` builds a complete `$8000`-entry predicate table. It follows
Acts-Like roots and recognizes `$01F`, `$020`, `$027`, `$028`, `$137`, `$138`, and `$13F`, plus
`$09C` in level mode `$01`; source/root `.dsc` flag bit 8 is additive. It then calls
`CheckLevelForInvalidExitDestinations`, which builds the packed screen-exit array and scans Layer 1
plus object-backed Layer 2 (excluding tilemap/background storage).

`MarkScreensWithInvalidExitDestinations` walks the materialized Map16 cells, maps each cell to one
of 32 screens, ignores screens without an exit, resolves the packed direct/midway destination, and
follows secondary-exit indices through the complete table. Secondary entries marked as overworld
destinations are skipped. Resolved levels `$000` and `$100` mark the source screen once.

Rust routes the command through the same staged level controller used by the canvas. Focused tests
bind direct, secondary, Acts-Like, `.dsc`, absent, valid, and overworld cases; the authenticated
toolbar partition test binds the command-table route.
