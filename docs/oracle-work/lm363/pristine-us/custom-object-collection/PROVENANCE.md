# Lunar Magic 3.63 custom-object collection preview

This fixture was captured on 2026-08-08 by
`rust_multi_object_collection_reloads_renders_and_places_in_lunar_magic` from the authenticated
Lunar Magic 3.63 executable and pristine headered North American SMW ROM.

- Lunar Magic SHA-256: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- pristine ROM SHA-256: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- preview SHA-256: `cd248183f65b1efbd6eea42714ee0464e23c08e1310f5d17ed00e5da48a9adb5`
- preview shape: 520×520, 8-bit RGBA PNG

The test authored one `.mw0` collection containing a screen-jump anchor and two relative custom
objects, with the paired `.mw0t` description `Rust multi-object placement oracle`. A Win32 helper
selected that exact entry in the live Add Objects window. The macOS ScreenCaptureKit helper then
captured only that Wine-owned window, clipped to the original preview-control rectangle, without
depending on foreground-window ordering. The retained image shows both separated rendered door
artworks.

After capture, the helper right-clicked the level canvas at `(96,96)`, saved, accepted Lunar
Magic's undefined-exit warning, and closed the editor. Lunar Magic's own before/after MWL exports
differed by exactly the two placed records `[06 06 10]` and `[07 0E 10]`; header, Layer 2,
sprites, palette, secondary exits, ExAnimation, and expanded settings remained equal, and the ROM
checksum reopened valid. The nonce-scoped ROM and sidecars are not retained.

The companion `lunar_magic_hides_a_custom_description_without_its_final_newline` run used a
well-formed one-entry `.mw0` and the byte-exact text `incomplete` without a final LF. Ghidra's
`PopulateCustomObjectTemplateList` shows that Lunar Magic commits list rows only on LF. The live
custom category consequently contained zero entries, Lunar Magic closed normally, and the ROM
remained byte-identical to the authenticated pristine input. Rust retains that final line for exact
round-trip editing but exposes `lunar_magic_picker_entries()` to reproduce the original picker.
