# Lunar Magic 3.63 ExLoROM overworld ExAnimation runtime oracle

This observation was captured on 2026-08-12 from the authenticated 32-bit original editor under
Wine, using a copier-headered 8-MiB ExLoROM baseline retained outside the repository. Proprietary
ROM bytes are not retained. Test assets contain only a compact differential against the previously
retained Lunar Magic `$C20` runtime template and adjacent Lunar Magic-owned RATS records.

- `Lunar Magic.exe` SHA-256: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Copier-headered input SHA-256: `b1ee089c0426eb06e3ad4b37c4c36e54df6496ea389fb2418a92e6ef384c21be`.
- Saved output SHA-256: `52a4c8dec612b161cd9cc9bd59ac2f3a5074237af8fca777f5cb26a8f1eadd41`.

The retained helper `tools/wine-overworld-animation-runtime-oracle.c` opened level `$105`, opened
the Overworld Editor and its extended-animation dialog, selected Type 1, Destination `$00A0`,
Frames `$00`, source `$0500`, accepted, and saved. `WINEDLLOVERRIDES=winevulkan=d` prevented
Wine's Vulkan path from crashing the GDI-only Overworld window. The uniquely named process was
targeted by exact PID, leaving the user's existing Lunar Magic process untouched.

After subtracting the copier header, Lunar Magic installed:

- runtime header `$204E32`, payload `$204E3A`, length `$C20`;
- auxiliary header `$205A5A`, payload `$205A62`, length `$15`;
- options header `$205A77`, payload `$205A7F`, length `$07`;
- compact-animation header `$205A86`, payload `$205A8E`, length `$11`.

The runtime payload SHA-256 is `9c769ddd9968f252e7601ac1f08e4cf543c1616d2a7856201235ce6fb858b8cf`.
Its 395-byte, 28-run delta has SHA-256
`b1dfee167ec41e5387bac1a029b291b067d9b289c85daa8207f81fc9cb2f9ca0`. The adjacent authentic
auxiliary/options owners have SHA-256
`ff782675edaf6fc5a4cb5a83eae26799467dc6ffd67640b00b06d4c2592d0328`.

This capture exposed a mapper-specific boundary: ExLoROM replaces four bytes at active-body
`$4200E0` with JSL `22 3A D3 C0`. Rust formerly wrote and decoded only a three-byte operand there.
`authentic_lunar_magic_363_exlorom_runtime_matches_every_owned_and_fixed_byte` reconstructs and
hashes the authentic payload, forces the exact allocation coordinates, normalizes only the edited
first auxiliary pointer, and compares every core-owner and fixed-hook byte. The broader row remains
Partial pending authentic SA-1 and `$C40` mapper-generation output evidence.
