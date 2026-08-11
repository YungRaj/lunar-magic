# Recent Files popup provenance

## Authenticated command surface

Lunar Magic 3.63 assigns command `$23DB` (`LM_FILE_RECENT_MENU`) to central dispatcher case
`$3D`; the executable command-dispatch table byte at file address `$004989AE` is `$3D`. The
recovered case refreshes menu state, builds a popup with `FUN_004790A0`, and invokes
`TrackPopupMenu` at the current pointer coordinates before destroying the temporary menu.

The recovered popup builder adds at most ten recent-file entries, using command IDs beginning at
`$23DC`. With no recent files it adds one disabled `$23DC` entry. With one or more entries it adds
their bounded paths followed by a separator (`$23D9`) and Clear Recent Files (`$23DA`). The
executable retains the corresponding English strings `&Recent Files`, `Open a Recent File`,
`&Clear Recent Files`, `Clear Recent Files List?`, and the confirmation sentence
`This will clear your recent files list. Are you sure you want to do this?`.

The decompiler evidence is retained locally in `/tmp/lm363-ghidra-recent-menu.log`; the proprietary
executable and ROM are not copied into this repository.

## Native publication boundary

The Rust user-toolbar route opens a pointer-anchored popup rather than a substitute dialog. It
shows up to ten existing recent paths; the empty state is disabled; the populated state includes
the separator and clear action. Selecting a path delegates to the existing recent-ROM lifecycle,
including dirty-project confirmation and bounded recent-state persistence. Clearing requires the
original title and confirmation sentence before publishing an empty persisted list. Escape and an
outside click dismiss the popup without changing application or project state.

Focused tests bind pointer placement and dismissal, selection through the established recent-ROM
open workflow, and the confirmation/persistence boundary. The complete authenticated command
partition test proves `$23DB` is routed exactly once.
