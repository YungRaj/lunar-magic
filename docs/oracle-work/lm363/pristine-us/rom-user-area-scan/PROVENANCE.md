# ROM user-area scan oracle

Captured on 2026-08-10 from Lunar Magic 3.63 in an isolated Wine prefix. The input was a
headered, 2-MiB SMW-US ROM already attributed to Lunar Magic 3.63. Neither the executable nor ROM
is retained here.

The normal capture records resource `$0427` after File → Scan ROM. For the conflict capture, a
valid 40-byte-payload `STAR` structure was placed at logical PC `$100000` and a valid 8-byte-payload
structure at `$100010`. Their complete ranges are `$100000..$100030` and `$100010..$100020`.
The copier header moves the physical log addresses to `$100200` and `$100210`.

Static evidence identifies scanner `$004A8F60`, dialog/caller `$00490C20`, and the resource launch
near `$00490F83`. The original conflict format string is at image `$005C0AF0`.

The Rust implementation deliberately identifies itself as Lunar Magic Rust in newly appended log
records; the field order, offsets, sizes, and conflict message shape match the retained original.
The historical pre-1.64 untagged Map16 allocation requires a separate authentic fixture before its
automatic locator can be counted as proven.
