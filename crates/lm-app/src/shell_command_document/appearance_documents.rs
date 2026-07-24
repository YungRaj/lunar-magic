use super::super::{
    EntityAppearanceDocumentCommand, OverworldAppearanceDocumentCommand, ShellCommand,
    ShellCommandError, no_argument, path_argument,
};

pub(in super::super) fn parse_overworld_appearance_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = ShellCommand::OverworldAppearanceDocument;
    let result = match command {
        "world-app-open" => path_argument(argument, "world-app-open")
            .map(OverworldAppearanceDocumentCommand::Open)
            .map(wrap),
        "world-app-edit-file" => path_argument(argument, "world-app-edit-file")
            .map(OverworldAppearanceDocumentCommand::Edit)
            .map(wrap),
        "world-app-undo" => no_argument(
            argument,
            "world-app-undo",
            wrap(OverworldAppearanceDocumentCommand::Undo),
        ),
        "world-app-redo" => no_argument(
            argument,
            "world-app-redo",
            wrap(OverworldAppearanceDocumentCommand::Redo),
        ),
        "world-app-status" => no_argument(
            argument,
            "world-app-status",
            wrap(OverworldAppearanceDocumentCommand::Status),
        ),
        "world-app-save" => no_argument(
            argument,
            "world-app-save",
            wrap(OverworldAppearanceDocumentCommand::Save),
        ),
        "world-app-close" => no_argument(
            argument,
            "world-app-close",
            wrap(OverworldAppearanceDocumentCommand::Close),
        ),
        "world-app-discard" => no_argument(
            argument,
            "world-app-discard",
            wrap(OverworldAppearanceDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(in super::super) fn parse_entity_appearance_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = ShellCommand::EntityAppearanceDocument;
    let result = match command {
        "entity-app-open" => path_argument(argument, "entity-app-open")
            .map(EntityAppearanceDocumentCommand::Open)
            .map(wrap),
        "entity-app-edit-file" => path_argument(argument, "entity-app-edit-file")
            .map(EntityAppearanceDocumentCommand::Edit)
            .map(wrap),
        "entity-app-undo" => no_argument(
            argument,
            "entity-app-undo",
            wrap(EntityAppearanceDocumentCommand::Undo),
        ),
        "entity-app-redo" => no_argument(
            argument,
            "entity-app-redo",
            wrap(EntityAppearanceDocumentCommand::Redo),
        ),
        "entity-app-status" => no_argument(
            argument,
            "entity-app-status",
            wrap(EntityAppearanceDocumentCommand::Status),
        ),
        "entity-app-save" => no_argument(
            argument,
            "entity-app-save",
            wrap(EntityAppearanceDocumentCommand::Save),
        ),
        "entity-app-close" => no_argument(
            argument,
            "entity-app-close",
            wrap(EntityAppearanceDocumentCommand::Close),
        ),
        "entity-app-discard" => no_argument(
            argument,
            "entity-app-discard",
            wrap(EntityAppearanceDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}
