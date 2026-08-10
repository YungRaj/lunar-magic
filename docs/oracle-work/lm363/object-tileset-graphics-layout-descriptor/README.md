# Object-tileset graphics layout-descriptor evidence

Authenticated target: Lunar Magic 3.63, Ghidra program `Lunar Magic.exe`, MCP port 8089.

`ProcessRomImageOpenTransaction` (`0047D000`) loads the tables used by
`LoadTilesetGraphicsFileAssignments` (`0049DD90`). The retained instruction window proves the
object-tileset table is not tied to the pristine SMW-US offset:

- `0047D036`: destination `ECX = 00816AA8`, Lunar Magic's live 16-by-4 FG/BG assignment table.
- `0047D040`–`0047D046`: source offset is loaded from active ROM-layout descriptor field `+0x94`.
- `0047D04C`–`0047D052`: the selected ROM stream seeks to that descriptor-provided offset.
- The preceding `EDX = EBX + 0x64` supplies the complete 64-byte table extent.

`LoadTilesetGraphicsFileAssignments` subsequently indexes `00816AA8` as
`object_tileset * 4 + slot`, for four slots and sixteen object tilesets. Rust therefore models this
as identity-bound profile metadata. Installed workflows reject profiles that omit it instead of
falling back to `SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET`; the direct authenticated SMW-US route
retains its separately proved pristine lookup.

See `disassembly.tsv` for the exact retained instruction sequence.
