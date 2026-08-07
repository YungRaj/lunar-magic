# Lunar Magic 3.63 undo-history limit oracle

Captured on 2026-08-07 from the authenticated local `Lunar Magic.exe` under Wine. Each row used an
isolated Wine prefix, wrote the named `UndoMain` registry value under
`HKCU\Software\LunarianConcepts\LunarMagic\Settings`, opened the verified pristine SMW-US ROM, and
read the live 32-bit level and overworld effective-limit globals at `$005E7734` and `$005E477C`.
The executable was copied to the unique process name `LMUndoOracle.exe` so concurrent Lunar Magic
sessions could not be sampled accidentally. `limits.tsv` records the complete recovered UI boundary
set and the above-maximum clamp. A normally closed default-prefix session independently contained
`UndoMain=$21`, and a live normal session reported `$21` in both editor globals. The automated
boundary gate verifies the fresh-prefix runtime default but does not claim registry publication
after its intentionally forced isolated-process shutdown.

The ignored integration gate
`undo_history_wine::original_lunar_magic_shares_and_clamps_every_undo_history_boundary` recreates
the isolated prefixes, compiles the committed read-only Wine process helper, and verifies every row
against the original executable. It is ignored by default because it requires Wine, 32-bit MinGW,
the locally licensed Lunar Magic 3.63 executable, and the verified pristine ROM.
