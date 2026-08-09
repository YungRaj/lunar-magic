# Lunar Magic 3.63 Map16 bitmap-import control inventory

This inventory is a clean-room record of the complete user-editable option surface recovered from
the labeled Lunar Magic 3.63 executable through the local Ghidra MCP service at port 8089. It
contains no Lunar Magic resource bytes.

The main preview procedure `HandleBitmapImportPreviewDialog` at `004F3A70` opens color resource
`$418` with procedure `004F15E0` from command `$6B`, opens other-options resource `$419` with the
previously unnamed procedure at `004F1FA0` from command `$74`, recomputes both previews after either
accepted subdialog, and commits only after command `1`. Command `2` cancels the conversion.

The callback at `004F1FA0` independently reads all six checkboxes and four hexadecimal values on
OK. Graphics values are accepted only below `$300`; Map16 values are accepted only below `$10000`.
Its initialization path writes each persisted value back into the corresponding control. The color
procedure independently reads its six checkboxes, reduction selector, 1–4 priority selector,
1–128 maximum-color selector, and 128-entry palette-state grid. Cancel restores both the complete
palette-state and color snapshots.

`controls.tsv` names every editable semantic control once and binds it to the Rust option field,
the original default, and the original range. The disabled exact-match checkbox is retained as a
fixed-on semantic rather than represented as a gesture the original user cannot perform.
