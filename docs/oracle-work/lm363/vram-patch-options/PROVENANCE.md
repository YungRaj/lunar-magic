# Lunar Magic 3.63 per-ROM VRAM patch options provenance

This record binds the clean-room Rust implementation of `LM_OPTIONS_VRAM` `$24E8` to observable
Lunar Magic 3.63 behavior and extracted data. The proprietary executable and ROM fixtures are not
added by this record.

## Authority

- Lunar Magic executable SHA-256:
  `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
  (3,162,112 bytes).
- Ghidra program: the labeled Lunar Magic 3.63 project served on port 8089.
- Live ordinary-LoROM dialog: title `Change VRAM Patch Options`; group `$65`; None `$294`;
  Normal `$295`; HD 16:9 `$296`; HD 21:9 `$297`; OK `$1`; Cancel `$2`.
- Extracted CHM topic: `html/option_vram.htm`.

## Runtime resource

- PE custom resource type 500, ID `$1FD`.
- RVA `$AB2E6C`, executable file offset `$276E6C`, total size `$3AB0` (15,024 bytes).
- Raw resource SHA-256:
  `cbbc98f922f09efb3be8e51be9a0da826d08ea8dba8cf185bd095e5f7e0e49c9`.
- Runtime length `$3390`; relocation metadata length `$720`.
- Metadata signature `LMRELOC1`, template origin `$1F8000`; sections `EXPA`, `EXT1`, `EXT3`,
  `TAB3`, `INT2`, `INT3`, `OPT3`, `REND`.
- The repository stores a gzip/base64 encoding of this authenticated resource at
  `crates/lm-profile/src/assets/vram_patch_normal_lm363.bin.gz.b64`, SHA-256
  `bd80cb210f16b97b652d1b1b9e0d8b3955d1062f7097f0c579d81a8ece578afb`.

## Static behavior

- `CheckInstalledVramPatchCompatibility` authenticates a RATS-owned primary JML payload by the
  final bytes `4C 4D 15 01` (`LM`, generation `$0115`). Generation `$0114` is recognized for
  replacement. Unknown ownership, magic, version, or future generation disables all choices.
- `InstallVramPatchRuntime` at `$00469B10` loads and relocates resource `$1FD`; its fixed-write
  tail spans `$00469D53..$0046A1C3`.
- `CheckVramPatchSignatureByte` is at `$00469880`; the version writer called after installation is
  at `$004698C0`.
- Active headered descriptors identify primary hook `$000003E2`, secondary hook `$000027A2`, and
  version byte `$000801E6`; Rust removes the `$200` copier prefix.

## Retained transition evidence

- Headered pristine before SHA-256:
  `5e3d55b019dd012e8db1498dda06b63ad1a304787625402b511e6d525946beaf`.
- Headered first-save after SHA-256:
  `9363981e0e902b00336184d9f2307773631a3b2bb89524cd4fb60c8c9db53882`.
- Installed logical RATS header `$08095A`; payload `$080962..$083CF2`.
- Installed payload SHA-256:
  `614c0c3736ef8f885dc1614b68e03ed8f1483238a02422476b33f49785aadeeb`.

Rust tests independently decode the embedded resource, reproduce the exact retained payload at
that address, relocate it at another bank address, validate every fixed hook/branch, detect
absent/current/replaceable/unknown states, compose installation with a prepared level save,
reopen the result, and restore the complete input with one Undo.
