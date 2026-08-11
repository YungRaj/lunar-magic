# Deprecated Options command no-op provenance

Lunar Magic 3.63 retains three historical internal command names whose central-dispatch table
entries are byte `$DF`:

| Command | ID | Dispatch-table address |
| --- | ---: | ---: |
| `LM_OPTIONS_CUSTOM_SPRTES` | `$24C5` | `$00498A98` |
| `LM_OPTIONS_WHEEL_ZOOM` | `$24DC` | `$00498AAF` |
| `LM_OPTIONS_ZOOM_MENU` | `$24DD` | `$00498AB0` |

The central byte table is addressed as `command_id + $004965D3`.
`HandleLevelEditorCommand` implements cases `$00..$DE`; `$DF` therefore reaches no command body
and returns through the original successful no-op boundary. These names must not be aliased to the
active sprite library or native canvas zoom behavior merely because their historical labels sound
related.

Rust routes all three entries to one explicit typed no-op. Focused coverage opens a pristine ROM
and requires the complete bytes, revision, status, and error state to remain unchanged. The
authenticated command-partition tests require every entry to be classified exactly once.
