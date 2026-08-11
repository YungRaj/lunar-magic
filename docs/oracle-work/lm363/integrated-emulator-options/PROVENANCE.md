# Integrated emulator option commands provenance

- Lunar Magic executable: `lm363/Lunar Magic.exe`, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Authenticated user-toolbar entries: slots 56, 57, 59, and 60; command IDs `$23CC`, `$23CD`,
  `$23CF`, and `$23D0`.
- Central dispatcher: `HandleLevelEditorCommand` at `$00492B80`.
- Disposable Ghidra 12.1.2 project: `/tmp/lm363-ghidra-emulator.beCZ4j`; retained analysis log:
  `/tmp/lm363-ghidra-emulator.log`.

The central byte table is addressed as `command_id + $004965D3`. The authenticated executable
bytes at `$0049899F` are `31 32 33 34 35`, mapping the consecutive Use-F4, selected-tiles,
frame-advance, paused-translucency, and stop-on-level-change commands to recovered switch cases
`$31..$35`.

- Case `$31` toggles whether F4 selects the internal or configured external emulator and reports
  `F4 changed to internal emulator.` / `F4 changed to emulator.`
- Case `$32` toggles selected editor tiles over the internal-emulator frame and reports the exact
  positive/negative status sentence.
- Case `$34` toggles half-transparent display for every paused internal-emulator mode.
- Case `$35` toggles stopping, rather than switching, the running internal emulator when the level
  changes.

Rust routes all four authenticated commands to one persisted native option model. Unmodified F4
honors the selected target after higher-priority user-toolbar assignments; selected-tile state is
shared bidirectionally with the canvas control; paused frames receive half alpha without changing
running frames; and a level transition either stops or switches the live session according to
the option. Focused tests bind every route, both state defaults, exact status text, persistent
save/reopen, canvas-state ownership, pause-mode opacity, and simultaneous level/revision changes.

The Windows cross-build passes. The complete native suite passes 1,043 tests with 13 explicit
external-fixture ignores. A fresh audit captures all 512 vanilla slots and emits a 513-line
manifest. Retained Lunar Magic exports confirm the sampled normal `$105`, vertical `$108`, and
unusual `$02D` content; a disposable `c20496c` build also reproduces the same `$02D` viewport. The
retained semantic renderer manifest remains byte-identical at SHA-256
`254a1a050d12785973241910e26d8b7917a5cb5e2a56602a330fc6cbd833c04d`.
