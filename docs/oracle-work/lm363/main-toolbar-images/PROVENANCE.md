# Lunar Magic 3.63 main-toolbar image oracle

The official 3.63 help topic `Custom Toolbar/GUI Images` specifies that a custom main-toolbar strip
is named `Lunar Magic.ff4`, is placed beside the executable, has one row of 41 square images, and
leaves image zero blank for the transparency key. A uniquely renamed clean Lunar Magic 3.63 process
was launched under Wine 11.13 (Staging) on 2026-08-06. `DumpLunarMagicToolbar.c` queried every live
`TBBUTTON`; the relevant initial command/image pairs were:

```text
command=9100 bitmap=1
command=9179 bitmap=2
command=9102 bitmap=3
command=9170 bitmap=4
command=9316 bitmap=5
command=9317 bitmap=6
```

The first group is Open/Open-level/Save/Save-as and Undo/Redo. The Rust default toolbar therefore
uses exact custom-strip cells 1, 3, 5, and 6 for its corresponding Open, Save, Undo, and Redo
actions. Buttons not yet represented by the native default toolbar do not invent mappings.

Original executable SHA-256:
`b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
