# Lfix3 generation-2 oracle evidence

This milestone used the official Lunar Magic 3.01 archive captured by the Internet Archive on
2019-01-17. The ROM and executable are clean-room oracle inputs and are not committed.

- Original URL: `https://fusoya.eludevisibility.org/lm/download/lm301.zip`
- Archived URL: `https://web.archive.org/web/20190117220628id_/https://fusoya.eludevisibility.org/lm/download/lm301.zip`
- Archive SHA-256: `2eacccec3d8770667bf496e14b8983586376dac7d5303528bc116571d91a4b8f`
- Executable SHA-256: `eb036287726ba87187aa875f37ab0bebdb21748c19e73503ca4c2177fcc927af`
- Oracle operation: open a pristine, headered SMW US v1 ROM; enable Super GFX Bypass to force
  expansion; accept the dialog; save the current level to the ROM.
- Resulting temporary ROM SHA-256: `354b0d07849c6d54e2c51e458fa8727ac9ecfc2b35b7ea249185fb53d8ed6562`

The result proves that generation 2 retains the generation-1 JSL at logical `$02D7CE`; therefore,
the simultaneous presence of both descriptor-selected hooks is valid and cannot be rejected as
ambiguous. The generation-2 primary hook at `$02DA17` owns a `$240`-byte RATS payload. The strict
detector authenticates that payload, its six relocated entry hooks, the two fixed helpers, the old
`LM $0110` table-helper marker, and the retained generation-1 hook. The three mutable 512-byte level
tables remain outside runtime authentication.

The recovered payload is stored as lowercase hexadecimal in
`crates/lm-profile/src/assets/lfix3_runtime_generation_2.hex`; its decoded length is `$240` bytes.
The ROM itself is deliberately absent from the repository.
