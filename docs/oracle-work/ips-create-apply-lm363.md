# Lunar Magic 3.63 IPS create/apply oracle

`tools/lunar-magic-ips-audit.sh` drives commands `$23BA` and `$23BB` in a uniquely named Lunar
Magic process and a private Wine prefix. The retained 2026-08-06 run used the verified headered
SMW-US-v1 ROM, changed logical byte `$001000` to `$42`, repaired its checksum, and compared all
published bytes.

| Observation | Result |
| --- | --- |
| Physical/logical input size | 524,800 / 524,288 bytes |
| Lunar Magic/Rust patch size | 23 / 23 bytes |
| Lunar Magic/Rust patch SHA-256 | `51aecf767b41f6e158d96d21a723da067c81f79b1d5e6b6fb329790961729b32` |
| Modified/applied physical SHA-256 | `2e5c017a8edda1ce89aed5e198fa4c7e1b6551a209818cfd527fcb773f7c09f5` |
| Physical changed-byte count | 5 |
| Modified-ROM Cancel | complete file unchanged |
| Malformed patch | complete file unchanged |

The exact shared patch is:

```text
50 41 54 43 48 00 12 00 00 01 42 00 81 DC 00 04 F3 5E 0C A1 45 4F 46
```

The `$001200` first record proves that Lunar Magic operates in canonical headered physical
coordinates rather than stripping the 512-byte prefix. The second record changes the four checksum
bytes at physical `$0081DC`. Lunar Magic's success dialogs reported `The IPS patch was successfully
created!` and `The file was successfully patched!`. Its modified-ROM confirmation exposed OK and
Cancel and warned that a non-original base could corrupt a full hack. Its malformed-file error was
`This is not an IPS file!`.
