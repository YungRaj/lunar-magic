# Lunar Magic 3.63 user-toolbar oracle

The fixture follows the official 3.63 help topic `Technical Information → Custom User Toolbar`.
It deliberately uses `LM_NO_TOOLBAR`: the original documents that this hides the second toolbar
while leaving its shortcut overrides active. The parser fixture also covers implicit definition
termination, a spacer, an internal command, an external command with quoted argument, five-line
fields, tooltip `\\n`, options, modifiers, executable-relative working directory, and global image
size/error directives.

Authoritative original implementation labels recovered from the Lunar Magic 3.63 Ghidra project:
`LoadUserToolbarConfiguration` (`004d4a50`), `ReadUserToolbarSourceLine` (`004d48a0`),
`ScanUserToolbarToken` (`004d49a0`), `AppendUserToolbarButtonDescriptor` (`004d39f0`),
`AppendUserToolbarIcon` (`004d43f0`), `LoadUserToolbarBitmapStrip` (`004d4570`), and
`CreateConfiguredUserToolbar` (`004d5b80`).

The canonical source file is UTF-8 and is loaded beside the executable at process startup. No ROM
is required for parsing or toolbar creation.

## Live Wine observation (2026-08-06)

Lunar Magic 3.63 was copied under the unique process name `LMToolbarOracle363.exe`, with
`usertoolbar-visible.txt` copied beside it as `usertoolbar.txt`, and launched under Wine 11.13
(Staging). `DumpLunarMagicToolbars.c` enumerated the live `ToolbarWindow32` children of `LMFrame`:

```text
toolbar hwnd=00000000000100B8 visible=1 count=52
toolbar hwnd=00000000000100C0 visible=1 count=2
```

Thus the original loaded the file at startup and created a distinct visible second toolbar. The
fixture's spacer is represented as toolbar separation and the two definitions produce two buttons.
The uniquely named oracle process was terminated after inspection without touching the user's
normal Lunar Magic process.

SHA-256 identities:

- Lunar Magic 3.63 executable: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- visible oracle fixture: `78e6cf6f2f3b889c0db59fb9d9f39674c4fc926f8bf1c6fc38212b572693e49f`
- hidden/complete parser fixture: `a76724ba30c0159d979c06427e093dfeaa83c53348800fb259bd0c116936ee54`
- toolbar enumeration helper source: `aea041a52e108aebd615965547280587dc30c605da24106d37ae1a85f0c6bd8b`

## Process launch policy evidence

The authenticated Lunar Magic 3.63 CHM (SHA-256
`6ff2a44ff32902aed11d1969970e2c19a91ef336c29795fed823b78e577d60be`) documents the exact
button contracts in `html/info_LM_options_button.htm` and the corresponding global contract in
`html/info_LM_options_global.htm`. `LM_ALLOW_MULT_INSTANCES` permits another process from the same
button instead of retaining a single tracked instance; `LM_ALLOW_MULT_INSTANCES_FORCE_ALL` applies
that policy to every button. `LM_NO_CONSOLE_WINDOW` requests a hidden console for directly launched
console programs and is explicitly inapplicable to `LM_OPEN_OTHER` ShellExecute launches.

The Rust launcher now retains every approved concurrent child independently, assigns a distinct UI
identity, supports cancelling every child owned by one button, and suppresses duplicate pending or
running requests under the default single-instance policy. On Windows only, the no-console option
adds `CREATE_NO_WINDOW` to the direct `std::process::Command`; it does not invoke a shell or affect
other platforms.

`LM_OPEN_OTHER` now takes the separate association-opening route promised by the same table. On
Windows a bounded UTF-16 `ShellExecuteW` wrapper receives the target, correctly quoted parameter
line, and optional working directory. macOS uses one direct `/usr/bin/open` process, and other Unix
systems use `xdg-open` while rejecting unsupported extra application arguments rather than dropping
them. Completion deliberately retains no opened-application child handle, so later close/notify
policies cannot falsely claim ownership.
