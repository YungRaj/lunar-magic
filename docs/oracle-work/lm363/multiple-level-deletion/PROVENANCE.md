# Lunar Magic 3.63 multi-level deletion oracle

The source is the retained authenticated copier-headered SMW-US revision-0 installed-level fixture
`oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc`, SHA-256
`507856670c4c6a0ef0aa187268287f754bb1ecacdfd418e83710b7dc3e3754e2`. Lunar Magic 3.63 x86 ran
under Wine against disposable copies. No ROM image is retained in this documentation directory.

| Command-line selection | Report | Output SHA-256 |
|---|---:|---|
| `-DeleteLevels oracle.smc -ModifiedLevels` | `Deleted 1 level.` | `08b427c9547c1881085a042d3ef341b6642d8139a912a099d6d9726815213ee3` |
| `-DeleteLevels oracle.smc -UnmodifiedLevels` | `Deleted 511 levels.` | `659d06f8662e716a98950eb82c0e84f66a89d521f63ee39e46b10d8ed31a819a` |
| `-DeleteLevels oracle.smc -AllLevels` | `Deleted 512 levels.` | `03af33fa60385f81d09f89a442866e7c4e2dcde2fc83393f0d46d701f423d7ad` |
| `-DeleteLevels oracle.smc -UnmodifiedLevels -ClearOrigLevelArea` | `Deleted 511 levels.` | `72b5bceeba2f764c2cf996c5133f84bb90433637743092e8b03daa44243a96d1` |
| `-DeleteLevels oracle.smc -LevelList 0,1` | `Deleted 2 levels.` | `5a675ad29e8e85ede57ff55efe47d86c3d18651242ccd19a075579aa77003596` |

Whole-image Rust tests require every hash above. The differentials additionally bind stable sorting
and duplicate removal, one revision and Undo step, redirection of all modeled per-level assets,
secondary-exit plane repacking/null-tail behavior, zero erasure of displaced owners, the exact
`$07EFC0..$07F09F` checksum-compensation run, checksum preservation, and save/reopen framing.

The bundled `file_delete_multiple_levels.htm` and `file_clear_level_area.htm` Help topics bind the
three category names, test-level replacement, the optional clear checkbox, protected original-area
islands, idempotent marker, and gameplay-critical-level warning. The clear differential installs
the marker owner at logical PC `$030258`, seven exact protected owners through `$040000`, moves the
test sprite low word to `$E76D`, clears expanded secondary exits, and writes the authenticated clear
metadata beginning at `$07EFC2`.
