# Lunar Magic 3.63 graphics extraction publication oracle

Source program: authenticated `Lunar Magic.exe` 3.63 in the labeled Ghidra project served at
`127.0.0.1:8089`. Static section mapping was independently checked with `llvm-readobj`: image
address `005B2D3C` maps to executable file offset `001B2D3C`, whose bytes are `77 62 00` (`wb`).

`ExtractAllGFXFiles` (`0047DA40`) opens every separate `Graphics/GFX%02X.bin` output with that
write/truncate mode. Joined mode, selected by parameter bit 0, opens `Graphics/AllGFX.bin` the same
way. `ExportExtendedGraphicsFromRom` (`0047EFF0`) uses the identical mode for each populated
`ExGraphics/ExGFX%02X.bin` or `ExGFX%03X.bin`. Existing regular files are therefore refreshed rather
than treated as collisions.

Rust keeps the original replacement outcome but strengthens publication: every decoded output is
staged before visibility, the complete fixed-name set replaces or creates atomically, existing
permissions are retained, and symlink/non-file destinations reject without partial replacement.
Single-file Save As extraction similarly honors the file dialog's overwrite decision through a
same-directory atomic replacement.
