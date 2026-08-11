# Deprecated Decrypt Levels command provenance

- Lunar Magic executable: `lm363/Lunar Magic.exe`, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Authenticated user-toolbar entry: slot 26, command ID `$23A5`,
  `LM_FILE_DECRYPT_LEVELS`.
- Central dispatcher: `HandleLevelEditorCommand` at `$00492B80`.

The dispatcher covers command IDs `$238D..$26FF` through the byte table addressed as
`command_id + $004965D3`. Entry `$23A5` is byte `$DF` at image address `$00498978`. The recovered
switch implements cases `$00..$DE` and has no `$DF` case, so this entry reaches the successful
default return without opening a dialog, changing settings, or mutating the ROM. This is distinct
from active restriction command `$23A4`, whose dispatcher byte is `$17`.

Rust routes the historical name to an explicit typed no-op. Focused coverage activates it with an
authenticated open ROM and requires identical revision and complete ROM bytes, no restriction
dialog, and no error. Treating it as an alias for restriction or as a hypothetical reversal would
contradict the original 3.63 dispatcher.
