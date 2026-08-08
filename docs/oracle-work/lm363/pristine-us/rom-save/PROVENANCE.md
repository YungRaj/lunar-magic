# Lunar Magic 3.63 ROM-save oracle

- Captured: 2026-08-07
- Original executable: 3,162,112 bytes, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- Input ROM: canonical headered pristine SMW-US, 524,800 bytes, SHA-256
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- Runtime: Wine Staging 11.13 in a new isolated prefix
- Process basename: `LMSaveOracle2.exe`

The recovered level-modified byte `$00E278D9` was set after the pristine ROM finished loading.
`WM_CLOSE` produced `Save level to ROM?`; clicking native control 6 (`&Yes`) opened
`Save Level to ROM as (in hex)`. Its native child controls are retained in `observation.tsv`.
The default expansion option remained selected and OK was clicked. Lunar Magic completed the
save, closed, and produced the exact 1,049,088-byte physical image recorded below. Rust inspection
proved a present copier header, 1 MiB logical LoROM, 13 RATS blocks, and a valid SNES checksum.

The separate retained `lifecycle-dirty-close` oracle supplies the rejection half: Cancel keeps the
frame and modified byte exact. Ghidra proves Lunar Magic does not expose a whole-ROM Save As:
`OpenRomBackingStream` (`004A69E0`), `WriteRomStream` (`004A6C10`), and
`CloseRomBackingStream` (`004A6AA0`) mutate and flush the selected ROM in place. Consequently there
is no original whole-ROM destination-collision gesture to capture; Rust's transactional Save As is
the stronger application workflow and its collision/failure matrix is tested independently.
