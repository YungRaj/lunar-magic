# Main and midway entrance dialog oracle

`dialog-configured.png` is a compositor capture of Lunar Magic 3.63's live
`Modify Main and Midway Entrance (in hex)` dialog while the authenticated pristine
SMW-US level `$105` is open under Wine. The oracle opens the dialog through recovered editor
command `$2524`, configures distinct main and separate-midway values across screen, X/Y, FG/BG,
Mario action, slippery, water, and facing controls, then captures the visible state before OK.

The ignored integration gate
`original_main_midway_dialog_applies_reopens_and_cancels_losslessly` compiles
`tools/wine-main-midway-entrance-oracle.c`, drives the original dialog, and establishes both
transaction boundaries:

- the Cancel transaction changes the same controls, enables separate-midway settings, and leaves
  every byte of the original ROM unchanged;
- the OK transaction saves and re-exports level `$105` through Lunar Magic itself, yielding main
  bytes `54 13 B7 1A C0 00 5A` and midway bytes `00 E9 0A 4B` in the typed MWL header view;
- the saved ROM has a valid checksum, and reopening the original dialog restores the separate
  checkbox plus the selected main and midway combo values;
- cancelling the reopened dialog leaves the already-saved ROM byte-identical.

Authenticated inputs:

- Lunar Magic 3.63 SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- canonical-header pristine SMW-US SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- retained PNG SHA-256:
  `9ff72defa995e52f716d7fc7d186bad5fd78f6b9f48b3ae79b6a81eee22e8fa7`

Regenerate and verify with:

```sh
LM_UPDATE_MAIN_MIDWAY_ORACLE=1 cargo test -p lm-app \
  --test main_midway_entrance_dialog_wine \
  original_main_midway_dialog_applies_reopens_and_cancels_losslessly \
  -- --ignored --nocapture
```
