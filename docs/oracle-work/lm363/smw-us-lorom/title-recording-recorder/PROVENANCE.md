# Lunar Magic 3.63 title-movement recorder oracle

- Captured: 2026-08-09
- Original executable: 3,162,112 bytes, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Runtime: Wine Staging 11.13 on macOS
- Harness: `tools/wine-title-recording-oracle.c`

The harness launches its own Lunar Magic process against a disposable ROM, locates only windows
owned by that exact process ID, opens the Overworld Editor with recovered command `$232D`, and
posts commands to its `OVFrame`. It never discovers or controls another Lunar Magic process.

Static dispatch recovery identified Overworld commands `$1F40` (save), `$1F44` (insert playback),
`$1F45` (export playback), `$1F46` (install recorder), and `$1F47` (uninstall recorder). Accepting
the native `$1F46` warning and saving produces the exact installed hash in `observation.tsv`.
The transaction replaces two hooks, publishes one 178-byte RATS-owned runtime, and reconstructs
Lunar Magic's bounded additive checksum-compensation run. Accepting `$1F47` and saving restores
the complete input byte-for-byte. A separate run clicks native Cancel control 2 at the install
warning and proves the complete ROM remains byte-identical.

The Rust profile test reconstructs the installed image independently and requires its complete
SHA-256, exact 347-byte delta count, semantic runtime reopen, Undo/Redo, and reciprocal uninstall.
The source ROM is not redistributed. This fixture closes the temporary recorder installation and
removal boundary for the authenticated SMW-US LoROM shape; emulator-driven recording and the
remaining playback file-dialog interactions stay open.
