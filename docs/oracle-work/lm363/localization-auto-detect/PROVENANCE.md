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

The complete transform was recovered headlessly from the labeled project on 2026-08-10. For byte
offsets whose bit 1 is clear, even offsets contribute `rol8(byte, 2) ^ $46`, while odd offsets
contribute the wrapping negation of `rol8(byte, 4) ^ $77`. Offsets whose bit 1 is set contribute
`u8(byte * -$80 + (byte >> 1) - $17) ^ $71`. Contributions accumulate with wrapping 32-bit
addition. The final 64 bytes never participate; the stored little-endian dword is at
`file_size - $38`.

`EnumerateAvailableLanguageModules` was also re-decompiled in full. It requires resource type
`$01F4`, ID `$0DB7` to contain at least the little-endian `$C001BABE` marker, then reads bounded
UTF-8 metadata from ID `$0DB6`, removes an optional BOM, normalizes CRLF delimiters, and retains
four fields after the module filename: display name, version, locale tag, and code page.

`LoadLanguageStringResources` at `$004D6D40` requires three more type-`$01F4` resources. `$0DAC`
is copied, then bytes 1 through end are decoded in place as
`((encoded ^ $92) - previous_decoded) + $34` before raw-DEFLATE expansion. `$0DAD` begins with a
little-endian declared count followed by 32-bit string offsets; `$0DAE` contains parallel 32-bit
lengths. The effective count is the minimum of the declaration, both complete table extents, and
`$16EE` (5,869). Each entry is retained only when offset plus length is in the inflated pool and
the following byte is NUL. `LoadSelectedLanguageModule` at `$004D7110` requires this string load
and the whole-file checksum before publishing the module.

The loader then applies two fixed-buffer safety tables before publication. `$005E6420..$005E685F`
contains 272 `(string index, exclusive byte-length ceiling)` pairs (raw-table SHA-256
`7c32c38900036af820ddda9311a66617c00c1882475182fd80a28358d7097903`).
`$005E6398..$005E641B` contains 22 `(inclusive start, exclusive end, exclusive byte-length
ceiling)` triples (raw-table SHA-256
`cabeed1bfddffaeca5550aababd4c59da60a901cae92b39f5a9f2950e279978e`). The loader clears both
offset and length when `length >= ceiling`; indices beyond the effective string count are ignored.
