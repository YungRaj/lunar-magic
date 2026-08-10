# Lunar Magic 3.63 user-toolbar internal command table

## Authority

- Executable: `lm363/Lunar Magic.exe`
- SHA-256: `b64998b637e553c9adb96dd893140b5b8d0303c7a0f46a1fdab5f887a1d46eff`
- PE image base: `$00400000`
- Ghidra loader: `LoadUserToolbarConfiguration` at `$004D4A50`
- Keyword lookup: `FindUserToolbarKeywordIndex`

The loader passes `$005E6A70` as the keyword-pointer base and bounds the search to `$013E`
(318) slots. It stores the selected original command ID from the parallel 16-bit table at
`$005E6F70`. In the executable's `.data` section these addresses map directly to raw file offsets
`$001E6A70` and `$001E6F70`.

The retained source fixture is
[`user_toolbar_internal_commands.tsv`](../../../../crates/lm-app/src/user_toolbar_internal_commands.tsv).
It records table slot, hexadecimal command ID, and keyword in original order. Slots `$000` through
`$13C` contain 317 named entries. Slot `$13D` is the null-pointer/zero-ID sentinel, so it is retained
as evidence but is not exposed as a command. `LM_EDIT_SELECT_ALL` intentionally occurs twice at
slots 71 and 90 with the same `$245D` ID. `LM_KEY_ADD_CSPRITE` and `LM_KEY_ADD_CUSTOM` are distinct
names sharing `$26AF`.

The table was extracted without executing Lunar Magic:

```sh
ruby -e 'b=File.binread(ARGV[0]); 318.times{|i| p=b.byteslice(0x1e6a70+i*4,4).unpack1("V"); s=""; if p != 0; o=p-0x400000; z=b.index("\0",o); s=b.byteslice(o,z-o); end; id=b.byteslice(0x1e6f70+i*2,2).unpack1("v"); printf("%03d\t%04X\t%s\n",i,id,s)}' 'lm363/Lunar Magic.exe'
```

## Rust boundary

`lm-app` exposes a typed lookup over the authenticated inventory. Tests require all 318 physical
slots, all 317 named entries in their original order, the terminal sentinel, duplicate-name and
shared-ID behavior, and successful `usertoolbar.txt` parsing of every named entry. `lm-native`
rejects invented internal names before dispatch, distinguishes an authenticated but not-yet-routed
command by its original ID, and currently routes 172 authenticated table entries to native commands,
level-view actions, or the same native workflow used by the corresponding menu. The direct workflow
set includes Help Contents/About, level analysis, restore-point creation/restoration, IPS
creation/application, the authenticated Sprite 19 installer, the integrated object/sprite placers
and their legacy aliases, return-to-level routing, integrated Layer 1/Layer 2/sprite editing modes,
selection-wide Select All/Delete/Delete All/Escape actions, and typed multi-record Cut/Copy/Paste
for Layer 1 objects, object-backed Layer 2, and sprites. This closes command enumeration and prevents
false acceptance, but does not claim that the remaining commands are implemented.
