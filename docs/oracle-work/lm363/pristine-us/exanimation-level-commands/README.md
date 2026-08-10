# Lunar Magic 3.63 level ExAnimation command recovery

The authenticated command table maps `$2530`, `$2531`, and `$2532` to
`LM_LEVEL_EX20_LEVEL`, `LM_LEVEL_EX20_GLOBAL`, and `LM_LEVEL_EX20_SETTINGS`.

In `HandleLevelEditorCommand` (`00492B80`), jump-table cases `$BA`, `$BB`, and `$BC` are contiguous:

- `$BA` calls `BeginEditingLevelExAnimations`, opens the modal editor, commits accepted level
  ExAnimation, reloads current-level graphics, and marks the level transaction modified.
- `$BB` performs the corresponding `BeginEditingGlobalExAnimations` and global commit path.
- `$BC` opens the settings dialog and reloads current-level graphics when accepted.

Rust's installed-ROM ExAnimation workspace already owns both revision-bound controllers, the
level/global switch, record/frame editing, triggers, slot setting/header fields, commit/reopen, and
dirty-close protection. These commands now open that workspace directly while retaining the level
editor as the primary mode. Level and settings start on the current level domain; global starts on
the global domain. A repeated route reuses a matching workspace and switches only when it is clean;
dirty or different-level work is never discarded.

The authenticated command partition test binds all three names and IDs. Existing ExAnimation
controller and ROM-editor tests cover record, frame, trigger, settings, domain, commit, and reopen
behavior.
