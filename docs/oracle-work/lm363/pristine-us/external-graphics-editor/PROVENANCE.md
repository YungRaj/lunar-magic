# Lunar Magic 3.63 external graphics-editor oracle

This oracle was captured from an isolated 32-bit Lunar Magic 3.63 process under Wine. It began
with the retained headered pristine SMW-US ROM, enabled Lunar Magic's exact custom-editor argument
setting, extracted the standard graphics, and drove the `Edit` button beside FG1 in the native
`Super GFX Bypass (in hex)` dialog. The capture did not use the user's running Lunar Magic process.

## Inputs and automation

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- pristine headered SMW-US SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- `tools/wine-graphics-editor-oracle.c` SHA-256:
  `e02d7fa8830dac708249588cc93192c1078b0534e45d9bb3aca68a4d97f05f42`
- compiled recorder SHA-256:
  `774ee39c9d05e66598f50ba5d5ad81c1bb294968d6142d106d4abdc22aebb5d3`
- `tools/wine-editable-combo-command.c` SHA-256:
  `d7126dba797fa0a68355e2ef5adf18437272d7fde8c4fbc9f0a9e0e8a31daab0`
- compiled dialog driver SHA-256:
  `da42616afd0414762e8b8c772499b620805f3cf73d199456cfd99f226b0a042b`

Lunar Magic stores the executable and argument template under
`HKCU\Software\LunarianConcepts\LunarMagic\Settings` as `TileEditor` and `TileEditorArg`.
`SynchronizeApplicationSettingsRegistry @ $0049BCA6` independently proves that bit 24 of the
`Options3` DWORD controls whether `TileEditorArg` is used. The isolated prefix therefore used:

```text
TileEditor  = Z:\tmp\lm-graphics-editor-oracle.exe
TileEditorArg = "Z:\tmp\lm-graphics-editor-argv.txt" "%1" "literal=%2"
Options3 = 0x01000000
```

The native command route was toolbar/menu command `$251E`, then dialog control `$0185`. The latter
maps to editable combo `$01EB`; the recovered dialog procedure reads its hexadecimal value and
calls `RenderLevelModeTilePreview @ $0040D140`, which calls
`LaunchGraphicsFileExternalEditor @ $00440B10`.

## Successful launch

The extracted `Graphics/GFX00.bin` began at SHA-256
`76eb1bac6a168cecfb11e37710dd0ed7d8b0416a6fc8fd060e67a653248bad36`, with first byte `00`.
The recorder received:

```text
argv[0]=Z:\tmp\lm-graphics-editor-oracle.exe
argv[1]=Z:\tmp\lm-graphics-editor-argv.txt
argv[2]=Z:\tmp\Graphics\GFX00.bin
argv[3]=literal=%2
```

The retained argument log SHA-256 was
`70d33bb086eb16ccfe588abaf97ba67daedd2b679fa2d2659c436a02f8d38f8d`. The recorder changed the
first graphics byte to `0F`; the resulting file SHA-256 was
`fa1dc418fb76027b4d62a5b8cea38af0c916557553c3b687c54d9a620160aa96`.

This proves that Lunar Magic replaces `%1` with the complete canonical graphics path, preserves an
unrecognized `%2` literally, and passes both through direct process arguments. The Super GFX
Bypass dialog remained open and responsive after the recorder exited. The recovered implementation
closes the process and thread handles immediately after `CreateProcess`, so the original neither
waits for the editor nor performs an automatic reload or reload prompt.

## Process-creation rejection

The same live dialog was then given FG1 value `01` after changing only `TileEditor` to
`Z:\tmp\does-not-exist-editor.exe`. Lunar Magic synchronously displayed:

```text
title: Couldn't Create Process!
message: "Z:\tmp\does-not-exist-editor.exe" "Z:\tmp\lm-graphics-editor-argv.txt" "Z:\tmp\Graphics\GFX01.bin" "literal=%2"
button: OK
```

After acknowledging the error, the Super GFX Bypass dialog remained open. This captures the
original rejection boundary and the exact fully expanded command reported to the user.

## Rust comparison

The Rust graphics workflow accepts both its native `{graphics}` placeholder and Lunar Magic's `%1`
alias only in direct graphics-editor arguments. It rejects either placeholder as a working
directory, stages the canonical graphics filename before launch, and reports launch failures
without publishing an edit. Unlike the original, Rust deliberately waits on a background worker
and reloads only a successful exact-size regular-file result; this is a stronger safe-edit boundary,
not an original reload prompt that remains to be reproduced.
