# Lunar Magic 3.63 SA-1 overworld ExAnimation runtime oracle

This observation was captured on 2026-08-12 from the authenticated 32-bit original editor under
Wine, using a copier-headered SA-1 Pack v1.40 SMW-US fixture. Proprietary ROM bytes are not
retained. Test assets contain only a compact differential against the retained Lunar Magic `$C20`
runtime template and adjacent Lunar Magic-owned RATS records.

- `Lunar Magic.exe` SHA-256: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`.
- Copier-headered input SHA-256: `f284c7df2d37c7b9e20f41e35440088b19f5eb2f76ce8248df592124ce7aeb6c`.
- Saved output SHA-256: `7f04fef2df8f00f9bd7eed042c3e0ccdb45ae3d72b851fee6a3e1c14fc9db13d`.

The helper acknowledged Lunar Magic's expected modified-ROM warning before posting the Overworld
command, then opened the extended-animation dialog, selected Type 1, Destination `$00A0`, Frames
`$00`, source `$0500`, accepted, and saved. Posting the Overworld command while that modal warning
was active caused Wine to fault; the corrected ordering follows the human interaction boundary.

After subtracting the copier header, Lunar Magic installed:

- runtime header `$084E32`, payload `$084E3A`, length `$C20`;
- auxiliary header `$085A5A`, payload `$085A62`, length `$15`;
- options header `$085A77`, payload `$085A7F`, length `$07`;
- compact-animation header `$085A86`, payload `$085A8E`, length `$11`.

The runtime payload SHA-256 is `9dae7a4fe84034935ebf62835a6f57d05d0d39740a6ac0d4f00ea7580411f971`.
Its 395-byte, 28-run delta has SHA-256
`afda35b94982021bd1c2c8d055d69d16362165620b6885c742f2c01707cd32ac`; the auxiliary/options
owners have SHA-256 `b1aab30ec161ed808804c9f73e24404022e1dc18935865935f27d45d2e4e4eb0`.

The capture proves SA-1 uses the non-LoROM four-byte JSL form at all three fixed hooks:
`22 3A CE 10`, `22 2A D0 10`, and `22 3A D3 10`. Rust formerly treated hook C as a three-byte
operand for SA-1. `authentic_lunar_magic_363_sa1_runtime_matches_every_owned_and_fixed_byte`
reconstructs and hashes the authentic payload, forces the original allocation coordinates,
normalizes only the deliberately edited first auxiliary pointer, and compares every core-owner and
fixed-hook byte. The inferred `$C40` historical generation remains separate and unpromoted pending
an authentic output fixture.
