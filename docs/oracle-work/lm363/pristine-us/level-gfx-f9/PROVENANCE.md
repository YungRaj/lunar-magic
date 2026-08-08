# Lunar Magic 3.63 current-level GFX publication oracle

This retained observation was captured on 2026-08-08 from the repository's authenticated,
headered pristine SMW-US ROM. Lunar Magic 3.63 itself accepted its `Not enough room` prompt and
expanded the working copy from `0x80200` to `0x100200` bytes before the observation. The SHA-256
identities of the executable, pristine input, and expanded working ROM are recorded in
`oracle.tsv`.

The original executable's tooltip dispatch table maps command `$232A` to `Open "8x8 Graphics
Editor" Window`. A Wine process helper opened that window and posted raw `WM_KEYDOWN`/`WM_KEYUP`
virtual key `$78` (F9) directly to its `Window8x8` top-level window. Lunar Magic displayed
`Save level GFX to Graphics folder?`; command `$0006` selected **Yes**.

## Separate-file capture

Lunar Magic's own `-ExportGFX` operation first produced `Graphics/GFX00.bin` through
`GFX33.bin`. Those 52 files were retained as the baseline, then every file was replaced by an
equal-length `$A5` sentinel. Publishing level `$105` restored exactly these eight files:

`GFX00`, `GFX01`, `GFX13`, `GFX14`, `GFX15`, `GFX17`, `GFX1B`, and `GFX20`.

The remaining 44 files stayed entirely `$A5`. The semantic assignment order read from the
original working buffers is `14,17,1B,15,00,01,13,20`; the sorted mutation set above is the same
set. A second run reset every file to `$A5`, moved `GFX33.bin` aside, and confirmed F9. Lunar
Magic displayed `Couldn't open file!` and changed zero of the remaining 51 files, proving the
complete-set preflight occurs before publication.

## Joined-file capture

The baseline files were concatenated in numeric order into an exact `0x36D00`-byte
`Graphics/AllGFX.bin`. Command `$24BD` changed live byte `$00E278C0` from zero to one. The joined
file was then replaced by an equal-length `$5A` sentinel and F9 was confirmed again. The observed
output was byte-identical to an independently constructed expectation that copied only the eight
ranges listed in `oracle.tsv` from the baseline. Its observed and expected SHA-256 hashes match.

Finally command `$24BD` restored the setting byte from one to zero, so the shared Wine profile was
not left in joined mode. Temporary ROMs, sentinel files, and helper binaries are not retained; the
small TSV is the reviewable oracle artifact bound by an automated Rust regression test.
