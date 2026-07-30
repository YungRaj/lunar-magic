# Lunar Magic 3.63 legacy multi-file level fixture

These files were exported live from the 32-bit `Lunar Magic.exe` 3.63 under Wine using the pristine
US v1 SMW ROM and level `105`.

Lunar Magic normally emits its current binary MWL container. For this recovery capture, the
legacy-export mode byte at process address `0x00e278d7` was set to `1`, then command `0x2395`
(`Save Level to File`) was dispatched. The resulting files were moved together without modifying
their contents:

- `Level 105.mwl`: text manifest
- `Level 105.mw0`: Layer 1 payload
- `Level 105.mw1`: Layer 2 payload
- `Level 105.mw2`: sprite payload

The source level has no custom palette, so Lunar Magic correctly omitted `.mw3`.

Recovered Ghidra ground truth:

- `ExportLegacyMultiFileLevel` at `004796c0`
- `WriteLegacyTextLevelManifest` at `00479530`
- `SetLevelFilePathAndSidecarNames` at `004792f0`
- `WriteLevelSidecarFile` at `00479250`
- `ImportLevelFileAutoDetect` at `00477940`
- `InitializeDefaultExpandedLevelHeaderRecord` at `00461fc0`
- `SkipTextHeaderAndCommentLines` at `00476770`

SHA-256:

```text
1c68970b1967028f7c26f74b60d53461193c660dd78c1646f537fa383a8ea1d7  Level 105.mwl
35488f92c6534d5bf8215eada121f4bb528da510387e8c0e041dfcd8f9b62942  Level 105.mw0
5c5299dbab174fdbe87fbded36e3b03f4ed4536073ce103fc1d626908f52927d  Level 105.mw1
70f4c370b2026fe1aa749bb58a4565a401f3ebb0f19665e9b4edf89af72cfeb1  Level 105.mw2
```
