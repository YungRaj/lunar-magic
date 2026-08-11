# Lunar Magic 3.63 legacy `Bypass.lst` transfer oracle

The source executable is the repository-external clean Lunar Magic 3.63 binary documented by the
parent `lm363` evidence set. Its bundled CHM has SHA-256
`6ff2a44ff32902aed11d1969970e2c19a91ef336c29795fed823b78e577d60be`.

The authenticated internal command table assigns `$239B` to
`LM_FILE_EXTRACT_EXGFX_LIST` and `$239C` to `LM_FILE_INSERT_EXGFX_LIST`. The bundled help topics
`html/file_extract_bypass.htm` and `html/file_insert_bypass.htm` identify these as the old FG/BG/SP
GFX bypass-list backup/transfer commands and name the default file `Bypass.lst`.

On 2026-08-10, command `$239B` was posted to a live Lunar Magic 3.63 process under Wine with the
repository's `wine-window-command.c` helper. The native Save dialog was directed to a fresh file.
The result was exactly 1,024 bytes with SHA-256
`812893e90de4018f71f8024dfd78e3ae731815911d257b211820b80a843aa932`. The exact 1,024 bytes at
physical ROM offset `$07F400` in the loaded copier-headered oracle ROM produced the same digest.
Thus the file is the raw `$400`-byte table: it has no envelope, signature, level selector, or copier
prefix.

Ghidra's recovered `ImportLegacyExGfxBypassList` additionally proves that insertion validates and
commits all `$400` bytes and installs the required expanded-settings/feature-control support when it
is absent. Rust performs prerequisite installation, table replacement, checksum repair, any
required expansion, and semantic reopen as one revision-bound mutation.

The proprietary executable, CHM body, ROM, and extracted table are not redistributed here.
