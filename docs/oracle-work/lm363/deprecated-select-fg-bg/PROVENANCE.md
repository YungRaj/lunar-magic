# Deprecated Select FG/BG commands provenance

- Lunar Magic executable: `lm363/Lunar Magic.exe`, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Authenticated user-toolbar entries: slots 88 and 89, command IDs `$2473/$2474`, names
  `LM_EDIT_SELECT_FG` and `LM_EDIT_SELECT_BG`.
- Central dispatcher: `HandleLevelEditorCommand` at `$00492B80`.

The central byte table is addressed as `command_id + $004965D3`. Both authenticated executable
entries at `$00498A46/$00498A47` contain `$DF`. The recovered switch implements only cases
`$00..$DE`, so both commands take the successful default return without changing selection, mode,
status, dialogs, history, or ROM bytes. These historical names must not be aliased to the active
`LM_EDIT_EDIT_LAYER_1/LM_EDIT_EDIT_LAYER_2` routes.

Rust maps both names to one explicit typed no-op. Focused coverage requires the original route
pair, stable project revision and complete ROM bytes, unchanged status, and no error.
