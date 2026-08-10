# Open Level From Address evidence

Target: Lunar Magic 3.63, executable SHA-256
`b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.

`llvm-readobj --coff-resources` identifies dialog resource `1001` at data RVA `$A85328`, size
338 bytes, language 1033. Its decoded extended dialog template is 170×44 dialog units and has the
title `Open Level From Address (in hex)`, font `MS Shell Dlg`, standard OK (`1`) and Cancel (`2`)
buttons, edit control `$7F`, and static label `$80` with text
`PC address to open level (in hex)`.

The bundled 3.63 CHM has SHA-256
`6ff2a44ff32902aed11d1969970e2c19a91ef336c29795fed823b78e577d60be`. Topic
`file_open_address.htm` states that the value is an exact ROM offset for Layer 1 data absent from
the main pointer table; sprites, entrances, and background are not loaded; the displayed level
number remains the preceding ordinary slot; Save inserts the imported Layer 1 into that ordinary
`$000..$1FF` slot; and the raw source pointer/address is neither discovered nor repaired. The same
topic identifies `$30263` as one known unreferenced vanilla stream, which is retained as the
pristine-ROM integration fixture.

Rust evidence is provided by the resource-localization/parser tests in
`open_level_address_dialog.rs`, the `$30263` pristine editor-state test in
`vanilla_level_editor.rs`, and the Layer-1-only save/reopen/source-preservation tests in
`level_controller/tests.rs`.
