# Lfix3 generation-1 recovery evidence

This milestone used two independent clean-room inputs. A ROM produced by the official Lunar Magic
3.01 oracle retained the complete generation-1 helper, while named decompilation of Lunar Magic
3.63 recovered the legacy migration loop and its descriptor-selected table addresses. Executables
and ROMs are oracle inputs and are not committed.

The immutable generation-1 identity is the JSL `22 50 DC 05` at logical `$02D7CE` and its complete
`$30`-byte fixed helper at logical `$02DC50`. Generation 1 has no independently owned relocatable
RATS runtime. Rust therefore requires both regions byte-for-byte and deliberately excludes mutable
per-level tables from authentication. Partial or modified candidates reject.

`MigrateLegacyLfix3LevelTables` reads 512 legacy flag bytes from logical `$02F600` and 512 packed
bytes from logical `$02DE00`. For each level whose flag bit `$20` is clear, it moves packed bit
`$10` to bit zero of a new plane and clears that packed bit. When flag bit `$20` is set, it leaves
the packed byte unchanged and writes zero to the new plane. The resulting planes are written to
logical `$02DE00` and `$037C00` before the current runtime is installed.

Named decompilation also proves the migration calls `InstallLfix3CoreHooks`,
`InitializeLfix3RuntimeTables`, and `InstallLfix3Runtime` before publishing the converted planes.
Generation 1 therefore leaves the later core/runtime destinations in their pristine state: its
only immutable installation is the separately authenticated hook/helper pair. The Rust migration
requires every later fixed write and both new table destinations to retain their exact pristine
preconditions. It captures the live packed plane as an exact transactional precondition, performs
the recovered conversion, initializes the third plane to `$1A`, installs the current `$510`
runtime, repairs the checksum, and commits once. Corrupt destinations reject before mutation;
successful output authenticates as current, and undo restores the complete source byte-for-byte.

An attempted historical-binary cross-check followed the 2019 WiiDatabase article to its archived
stable `Lunar-Magic.zip` URL. The Internet Archive has no payload capture between the 2016 and 2021
digests, and replay returns a later executable dated 2020. That file was rejected as generation-1
evidence rather than silently treated as Lunar Magic 3.00.
