//! Authenticated Lunar Magic 3.63 `usertoolbar.txt` internal-command inventory.
//!
//! The original parser searches a 318-slot pointer table. Slots 0 through 316 name commands;
//! slot 317 is the null sentinel. Keeping the sentinel in the retained table makes the original
//! loop boundary independently testable without presenting it as a usable command.

pub const LUNAR_MAGIC_363_USER_TOOLBAR_TABLE_SLOTS: usize = 318;
pub const LUNAR_MAGIC_363_USER_TOOLBAR_NAMED_COMMANDS: usize = 317;

const COMMAND_TABLE: &str = include_str!("user_toolbar_internal_commands.tsv");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserToolbarInternalCommand {
    pub slot: u16,
    pub command_id: u16,
    pub name: &'static str,
}

pub fn lunar_magic_363_user_toolbar_commands() -> impl Iterator<Item = UserToolbarInternalCommand> {
    COMMAND_TABLE.lines().filter_map(parse_named_command)
}

pub fn user_toolbar_internal_command(name: &str) -> Option<UserToolbarInternalCommand> {
    lunar_magic_363_user_toolbar_commands().find(|command| command.name == name)
}

fn parse_named_command(line: &'static str) -> Option<UserToolbarInternalCommand> {
    let mut fields = line.splitn(3, '\t');
    let slot = fields.next()?.parse().ok()?;
    let command_id = u16::from_str_radix(fields.next()?, 16).ok()?;
    let name = fields.next()?;
    (!name.is_empty()).then_some(UserToolbarInternalCommand {
        slot,
        command_id,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UserToolbar, UserToolbarTarget};
    use std::collections::HashSet;

    #[test]
    fn retained_table_has_every_named_entry_and_one_null_sentinel() {
        let lines = COMMAND_TABLE.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), LUNAR_MAGIC_363_USER_TOOLBAR_TABLE_SLOTS);
        assert_eq!(lines.last(), Some(&"317\t0000"));

        let commands = lunar_magic_363_user_toolbar_commands().collect::<Vec<_>>();
        assert_eq!(commands.len(), LUNAR_MAGIC_363_USER_TOOLBAR_NAMED_COMMANDS);
        assert_eq!(
            commands
                .first()
                .map(|entry| (entry.slot, entry.command_id, entry.name)),
            Some((0, 0x238c, "LM_FILE_OPEN_ROM"))
        );
        assert_eq!(
            commands
                .last()
                .map(|entry| (entry.slot, entry.command_id, entry.name)),
            Some((316, 0x26ff, "LM_MOUSE_EDIT_SCREEN_EXIT"))
        );
        assert!(
            commands
                .iter()
                .enumerate()
                .all(|(slot, command)| usize::from(command.slot) == slot)
        );
    }

    #[test]
    fn lookup_preserves_original_duplicates_and_alias_ids_but_rejects_unknown_names() {
        let commands = lunar_magic_363_user_toolbar_commands().collect::<Vec<_>>();
        let unique_names = commands
            .iter()
            .map(|command| command.name)
            .collect::<HashSet<_>>();
        assert_eq!(unique_names.len(), commands.len() - 1);
        assert_eq!(
            commands
                .iter()
                .filter(|command| command.name == "LM_EDIT_SELECT_ALL")
                .map(|command| command.slot)
                .collect::<Vec<_>>(),
            vec![71, 90]
        );
        assert_eq!(
            user_toolbar_internal_command("LM_EDIT_SELECT_ALL")
                .unwrap()
                .command_id,
            0x245d
        );
        assert_eq!(
            user_toolbar_internal_command("LM_EDIT_SELECT_ALL")
                .unwrap()
                .slot,
            71
        );
        assert_eq!(
            user_toolbar_internal_command("LM_KEY_ADD_CSPRITE")
                .unwrap()
                .command_id,
            0x26af
        );
        assert_eq!(
            user_toolbar_internal_command("LM_KEY_ADD_CUSTOM")
                .unwrap()
                .command_id,
            0x26af
        );
        assert_eq!(user_toolbar_internal_command(""), None);
        assert_eq!(user_toolbar_internal_command("LM_UNKNOWN"), None);
    }

    #[test]
    fn parser_retains_every_original_named_internal_target() {
        for command in lunar_magic_363_user_toolbar_commands() {
            let text = format!("***START***\n{}\n***END***", command.name);
            let toolbar = UserToolbar::parse(&text).unwrap_or_else(|error| {
                panic!(
                    "slot {} ({}) did not parse: {error}",
                    command.slot, command.name
                )
            });
            assert_eq!(toolbar.buttons.len(), 1, "slot {}", command.slot);
            assert_eq!(
                toolbar.buttons[0].target,
                UserToolbarTarget::Internal(command.name.to_owned()),
                "slot {}",
                command.slot
            );
        }
    }
}
