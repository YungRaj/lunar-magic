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

The Rust conversion function and exhaustive 512-entry branch test preserve this behavior. The
remaining migration transaction still requires authentication of every generation-1 precondition
that Lunar Magic replaces; the application continues to refuse legacy installation until that
transaction is proven failure-atomic.
