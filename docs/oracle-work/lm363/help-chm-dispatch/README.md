# Lunar Magic 3.63 CHM dispatch oracle

This retained static fixture comes from the authenticated `Lunar Magic.exe` 3.63 program in the
labeled Ghidra project served at `127.0.0.1:8089`.

`OpenLunarMagicHelpFile` (`00440F90`) first tries the active language module with its extension
replaced by `.chm`, then falls back to `Lunar Magic.chm` beside the executable. It attempts to
remove the `:Zone.Identifier` alternate stream, calls `ShowHtmlHelpFromUtf8Path` (`004E4870`), and
reports a missing-file or launch error. That wrapper calls `HtmlHelpW`/`HtmlHelpA` with command and
data both zero, so the editor command opens the CHM contents; its parameter is the owner window,
not a topic route. A failed first launch retries the same request with an 8.3 short path at
`004E4B30`. The level and overworld command callers are at `00497E36` and `00564DB9`.

Rust retains the searchable, non-proprietary title/route index in-process and can now open an
installed adjacent `Lunar Magic.chm` through the platform help/file handler. It never bundles or
modifies the proprietary file, and rejects a missing, symbolic-link, or non-file sibling before
starting a process.
