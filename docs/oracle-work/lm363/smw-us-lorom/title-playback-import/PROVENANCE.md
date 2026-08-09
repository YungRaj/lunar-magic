# Lunar Magic 3.63 title playback import oracle

This observation was captured on 2026-08-09 from the authenticated local Lunar Magic 3.63 binary
and the authenticated 2 MiB SMW-US LoROM baseline named by `observation.tsv`. The PID-scoped
`tools/wine-title-recording-oracle.c` harness launches and controls only its own Lunar Magic
process. It opens the Overworld Editor with `$232D`, invokes `$1F44`, enumerates the confirmation
and common-file-dialog controls, and saves accepted state with `$1F40`.

The input is produced deterministically by `lm_title::encode_zsnes_title_recording` for movement
bytes `12 34 56 FF`. No ROM or emulator savestate is retained in the repository. Hashes, sizes,
logical offsets, dialog structure, and cancellation results are retained instead.

The original output was compared with a Rust import of the same generated state. The complete ROM
files are byte-identical, not merely semantically equivalent. This binds allocation order and
location, all runtime and pointer bytes, the unchanged physical length, the stored checksum, and
the compensation run. Confirmation Cancel and file-dialog Cancel each leave the input hash
unchanged.

The executable's authenticated `-ImportTitleMoves` and `-ExportTitleMoves` batch routes provide the
noninteractive file boundaries without opening a second editor window. Valid batch import produces
the same complete ROM hash as the GUI and Rust. Batch export recreates the 134,163-byte input state
byte-for-byte. A 12-byte truncation emits the retained `Not a ZSNES Savestate!` rejection, exits 1,
and leaves the ROM byte-identical. Export from the pristine ROM emits the retained
`ASM code not detected!` rejection, exits 1, and creates no output.

The same authenticated batch import was run against the byte-exact 524,800-byte copier-headered
vanilla ROM. Lunar Magic emitted its not-enough-room prompt, selected `YES`, expanded to exactly
1 MiB logical/1,049,088 physical bytes, initialized the recovered expansion metadata, installed
the movement payload/runtime at the expansion boundary, and retained the copier prefix. The Rust
transaction reproduces the complete output byte-for-byte at SHA-256
`662f1f980bb02f8ec2f6ac1be27835f7269091336f0f07008499afe6717c058c`; Undo restores the exact
vanilla image.

A follow-up import replaces the seven-byte installed payload with a 257-byte recording. It proves
that Lunar Magic zero-fills the reclaimed owner and that the additive compensation span continues
through logical `$07F09F` (exclusive end `$07F0A0`), rather than stopping before the metadata
padding. Rust again matches the complete copier-headered output at SHA-256
`46079b7e14c90d89cc7b46a797bd05a48fabacaec7fc6d7e63134bc405d36bb0`.
