# Lunar Magic 3.63 localization auto-detection oracle

This fixture records direct decompilation evidence from the labeled `Lunar Magic.exe` program on
the local Ghidra MCP server at port 8089. It contains no Lunar Magic binary or language-resource
payload.

`EnumerateAvailableLanguageModules` at `004D7940` adds built-in English as `(Default)`, scans only
`sysLMLanguage\\*.dll`, requires resource type `$01F4` IDs `$0DB7` and `$0DB6`, requires the first
resource to begin with little-endian `$C001BABE`, accepts at most `$410` bytes of metadata, removes
an optional UTF-8 BOM, and splits four newline-delimited metadata fields after the module filename.

`AutoDetectAndLoadLanguageModule` at `004D7360` treats persisted `(Default)` and `(AutoDetect)` as
distinct modes. Auto detection compares up to 64 preferred UI-language tags case-insensitively,
first in complete form and then after truncating each installed tag at its first hyphen. A match
persists the selected module filename; no match retains built-in English. `GetPreferredUiLanguagesAsUtf8`
at `004DB810` bounds the Windows preferred-language multi-string to `$600` UTF-16 units and falls
back through `BuildFallbackPreferredUiLanguageList` at `004DB640`.

`ValidateLanguageModuleChecksum` at `004D7010` decodes every byte before the final 64-byte trailer,
accumulates the recovered position-dependent transform, reads the stored 32-bit checksum at
`file_size - $38`, and unloads the module on mismatch. These details retain the original DLL ABI
for later compatible resource loading; the Rust catalog remains a bounded clean-room format.
