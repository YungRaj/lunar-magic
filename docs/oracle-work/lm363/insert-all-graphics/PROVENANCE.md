# Insert All Graphics provenance

- Lunar Magic executable: `lm363/Lunar Magic.exe`, SHA-256
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Pristine headered SMW-US input: `sysLMRestore/smwOrig.smc`, SHA-256
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`.
- Authenticated command: `LM_FILE_INSERT_ALL_GRAPHICS`, command ID `$23D7`, dispatch case
  `$3B` in `HandleLevelEditorCommand` (`$00492B80`).

The recovered case calls `InsertStandardAndExtendedGraphics` at `$0047FC30`. That function sets
the operation text to `Insert all GFX and ExGFX to ROM.`, opens one shared operation, invokes
`InsertAllGFXFiles` at `$0047E720` with `joined | flags | 6`, then invokes
`ImportExtendedGraphicsIntoRom` at `$0047F470`. Only after both succeed does it run the shared
finalization sequence and return the combined inserted-byte count plus one. A failure in either
phase returns zero, so the native route must not publish the standard phase independently.

The ignored live gate
`lunar_magic_import_all_graphics_and_atomic_rust_route_reexport_the_same_assets` runs Lunar Magic
3.63 `-ImportAllGraphics` and the Rust `$23D7` route from the same pristine ROM, 52 exported
standard files, and deterministic `ExGFX80.bin`. Both outputs retain the same supported identity,
physical length, and valid checksum. Lunar Magic then independently re-exports all 52 GFX files
and `ExGFX80` from both ROMs; every asset is byte-identical. Rust additionally proves one revision,
semantic reopen, and byte-exact Undo/Redo. The complete ROMs are not asserted byte-identical because
the two allocators validly choose different free-space placements.
