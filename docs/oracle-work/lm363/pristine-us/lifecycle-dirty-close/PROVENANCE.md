# Lunar Magic 3.63 dirty-close oracle

- Captured: 2026-08-07
- Original executable: 3,162,112 bytes, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Input ROM: canonical headered pristine SMW-US, 524,800 bytes, SHA-256
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- Runtime: Wine Staging 11.13 in a new isolated prefix, GDI renderer
- Process basename: `LMLifecycleOracle.exe`

The ROM was opened normally. The recovered level-modified byte at `$00E278D9` was set while the
loaded-state byte `$00E2782A` and prompt-enable byte `$005E7ADB` retained their live value `$01`.
Sending `WM_CLOSE` to `LMFrame` produced the retained native child-control observation in
`observation.tsv`. Clicking control `2` (Cancel) left the frame present and the modified byte `$01`.
Sending `WM_CLOSE` again produced the same prompt. Clicking control `7` (No) closed the process.

Port-8089 Ghidra independently recovers the coordinator and exact result handling:
`CheckCanProceedAfterCoreSavePrompts` at `$00455F50`,
`CheckCanProceedAfterAllSavePrompts` at `$00455F80`, and
`PromptToSaveModifiedLevel` at `$00491080`. Result `2` cancels; result `6` dispatches save command
`$2392`; any non-cancel result clears the level-modified state.
