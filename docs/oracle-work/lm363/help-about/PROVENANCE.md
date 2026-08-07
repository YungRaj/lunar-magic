# Lunar Magic 3.63 Help/About oracle

Captured live on 2026-08-07 from the authenticated local 32-bit executable under Wine Staging
11.13.

- `Lunar Magic.exe` SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- `Lunar Magic.chm` SHA-256:
  `6ff2a44ff32902aed11d1969970e2c19a91ef336c29795fed823b78e577d60be`
- The CHM table of contents contains 314 section/topic nodes at depths 1 through 4, including
  281 routed entries and 275 distinct HTML routes.
- Ghidra identifies `AboutDialogProc` at `00415970`, dialog resource `$03F8`, and the level-editor
  command dispatcher route `$25E5`.
- Posting command `$25E5` to the live `LMFrame` created one modal `#32770` window titled
  `About Lunar Magic...`. `about-controls.tsv` retains its nonempty identity, version, build, URL,
  action, and dismissal controls. The long legal disclaimer body is intentionally not retained.
- The live dialog was dismissed through control ID `1`; the modal window closed and the original
  `LMFrame` remained alive.

Capture commands used the repository's compiled `wine-window-command.exe` helper:

```text
post-command 0x25e5
children
click 1
```
