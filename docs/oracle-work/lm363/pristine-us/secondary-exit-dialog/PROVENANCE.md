# Secondary-exit dialog oracle

These compositor captures come from Lunar Magic 3.63's original
`Modify Secondary Entrances (in hex)` workflow on authenticated pristine SMW-US level `$105`:

- `configured.png` shows slot `$1FFE` configured with destination `$1AB`, screen `$1D`, distinct
  X/Y and FG/BG selectors, a pipe action, and all three destination flags.
- `clear-all-confirmation.png` retains the exact modal warning shown by Clear All while the saved
  slot is still visible behind it.

The ignored integration test
`original_secondary_exit_dialog_applies_clears_rejects_and_cancels_losslessly` compiles
`tools/wine-secondary-exit-oracle.c`, opens resource `$03F1` through command `$2525`, and proves:

- editing then Cancel restores the dialog's `$C000`-byte temporary table and leaves the pristine
  ROM byte-identical;
- OK saves slot `$1FFE` as the exact typed record `{ destination: 1AB, position/method: EB,
  screen: 1D, y: 6, destination flags: 84, additional flags: 60 }`, repairs the checksum, and
  restores every selected control after reopening;
- rejecting Clear All with No preserves the complete saved ROM byte-for-byte;
- Clear Slot clears exactly `$1FFE`, while accepting Clear All clears all 8,192 records;
- Lunar Magic's empty installed representation consists of four fixed zero planes and two null
  reader pointers. Rust accepts and reproduces that representation with no synthetic RATS owners.

Authenticated input hashes:

- Lunar Magic 3.63: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- canonical-header pristine SMW-US: `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`
- `configured.png`: `c6b58d835931e774aada92bb91ad56b24b4a95b3eae00e7e68ca05588fe8cb42`
- `clear-all-confirmation.png`: `a6b371b63fe8d43b73fa538da26d1b051358bf34ef2c299109c0f06fbc457a2a`

Regenerate and verify with:

```sh
LM_UPDATE_SECONDARY_EXIT_DIALOG_ORACLE=1 cargo test -p lm-app \
  --test secondary_exit_dialog_wine \
  original_secondary_exit_dialog_applies_clears_rejects_and_cancels_losslessly \
  -- --ignored --nocapture
```
