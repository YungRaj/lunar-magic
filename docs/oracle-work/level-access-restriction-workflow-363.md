# Lunar Magic 3.63 level-access restriction workflow

This note records the original post-mutation workflow used to verify the native Rust frontend. The
evidence comes from the labelled Lunar Magic 3.63 Ghidra program exposed by the local bridge.

## Dispatcher

`HandleRestrictLevelAccessCommand` at `00485050` performs the following operations in order:

1. increments the long-operation guard at `00e27980`;
2. calls `RestrictLevelAccessInRom`;
3. decrements the guard;
4. returns without post-processing when restriction failed;
5. uses the operation label `Restrict Level Access.`;
6. when the restore-point option at `005e628e` is set, calls
   `CreateFullRomRestorePoint` at `004850af`;
7. displays a Yes/No question (`MB_YESNO | MB_ICONQUESTION`, style `0x24`);
8. on Yes, sends `WM_COMMAND` `$23BA`, the standard Create IPS command;
9. displays the completion information box (`MB_ICONINFORMATION`, style `0x40`); and
10. calls `ResetLevelEditorAfterRomClose` at `00485136`.

`DecodeRomFeatureOptionFlags` at `004ad740` assigns input bit 0 to `005e628e`. Its only caller is
`SynchronizeApplicationSettingsRegistry` at `0049c220`, proving this is an application-settings
option rather than a bit stored in the open ROM. `EncodeRomFeatureOptionFlags` writes the same bit
back into the packed settings word. The option is also consulted by `PrepareAutomaticRestorePoint`
and other destructive bulk-save commands.

## Authenticated strings

Memory beginning at `005ba538` contains the IPS question:

- title: `Create an IPS patch?`
- body: `Do you want to create an IPS for this locked ROM?`

Memory beginning at `005ba59c` contains the final notification:

- title: `Level Access Restriction Complete`
- body: `Your modified levels are no longer accessible by Lunar Magic. Performing any additional operations on this ROM is not recommended.`

## Rust boundary

The native workflow now preserves the original observable order after a successful mutation:

`restriction/persist -> optional full restore point -> IPS offer -> optional standard IPS workflow -> completion notice -> close`

Because the Rust project model separates an undoable in-memory mutation from asynchronous atomic
file persistence, it waits for save acknowledgement before offering file-based IPS creation. A
failed save leaves the restricted project open and offers Retry Save; it never lets the IPS chooser
compare against stale pre-restriction bytes. Final close is also save-acknowledged, so a later failed
write leaves the project open with Retry Save and Close. The native restore preferences retain the
independent destructive-operation bit. When enabled, restriction first publishes the locked ROM,
then creates or appends a full record in its associated `.lrp` archive before IPS creation. The ROM
cannot advance to the IPS offer until both persistence and the checkpoint succeed.

Follow-up recovery corrected the earlier assumption that the archive itself is registry-backed.
`OpenOrCreateRestoreArchive` at `004AEFB0` calls `EnsureRestoreDirectoryExists` (`004AEF00`) and
`BuildRestoreArchiveFilename` (`004AED30`). The original creates `sysLMRestore` beneath the active
ROM's directory, publishes its explanatory `readme.txt`, replaces the active ROM filename's
extension with `.lrp`, and opens or creates that archive without a chooser. The three supported
original-ROM names selected by the active game/region are `smwOrig.smc`, `smwjOrig.smc`, and
`AllWorldOrig.smc`. Only the fallback original-ROM location is persisted as `OrigROMn`; the first
search is always the corresponding `sysLMRestore` file.

Rust now follows that association directly. Restriction creates or appends the full checkpoint in
`<ROM directory>/sysLMRestore/<ROM stem>.lrp`, reuses the variant-specific original copy without a
chooser, validates its game, region, revision, and checksum, and asks once only when that copy is
missing before publishing it under the original name. Archive publication remains create-new or
atomic replacement as appropriate. Portable tests bind all three supported names, dotted ROM
filenames, initial archive creation, repeated append, reconstruction, and the no-chooser reuse
path.
