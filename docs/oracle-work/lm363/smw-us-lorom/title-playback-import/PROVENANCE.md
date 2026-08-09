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
