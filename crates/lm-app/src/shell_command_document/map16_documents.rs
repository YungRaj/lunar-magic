use super::super::{
    Map16DocumentCommand, Map16PageDocumentCommand, ShellCommand, ShellCommandError, no_argument,
    path_argument,
};

pub(in super::super) fn parse_map16_page_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::Map16PageDocument(command);
    let result = match command {
        "map16-page-open" => path_argument(argument, "map16-page-open")
            .map(Map16PageDocumentCommand::Open)
            .map(wrap),
        "map16-page-edit-file" => path_argument(argument, "map16-page-edit-file")
            .map(Map16PageDocumentCommand::Edit)
            .map(wrap),
        "map16-page-render-file" => path_argument(argument, "map16-page-render-file")
            .map(Map16PageDocumentCommand::Render)
            .map(wrap),
        "map16-page-undo" => no_argument(
            argument,
            "map16-page-undo",
            wrap(Map16PageDocumentCommand::Undo),
        ),
        "map16-page-redo" => no_argument(
            argument,
            "map16-page-redo",
            wrap(Map16PageDocumentCommand::Redo),
        ),
        "map16-page-status" => no_argument(
            argument,
            "map16-page-status",
            wrap(Map16PageDocumentCommand::Status),
        ),
        "map16-page-save" => no_argument(
            argument,
            "map16-page-save",
            wrap(Map16PageDocumentCommand::Save),
        ),
        "map16-page-close" => no_argument(
            argument,
            "map16-page-close",
            wrap(Map16PageDocumentCommand::Close),
        ),
        "map16-page-discard" => no_argument(
            argument,
            "map16-page-discard",
            wrap(Map16PageDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(in super::super) fn parse_map16_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::Map16Document(command);
    let result = match command {
        "map16-set-open" => path_argument(argument, "map16-set-open")
            .map(Map16DocumentCommand::Open)
            .map(wrap),
        "map16-set-edit-file" => path_argument(argument, "map16-set-edit-file")
            .map(Map16DocumentCommand::Edit)
            .map(wrap),
        "map16-set-render-file" => path_argument(argument, "map16-set-render-file")
            .map(Map16DocumentCommand::Render)
            .map(wrap),
        "map16-set-undo" => {
            no_argument(argument, "map16-set-undo", wrap(Map16DocumentCommand::Undo))
        }
        "map16-set-redo" => {
            no_argument(argument, "map16-set-redo", wrap(Map16DocumentCommand::Redo))
        }
        "map16-set-status" => no_argument(
            argument,
            "map16-set-status",
            wrap(Map16DocumentCommand::Status),
        ),
        "map16-set-save" => {
            no_argument(argument, "map16-set-save", wrap(Map16DocumentCommand::Save))
        }
        "map16-set-close" => no_argument(
            argument,
            "map16-set-close",
            wrap(Map16DocumentCommand::Close),
        ),
        "map16-set-discard" => no_argument(
            argument,
            "map16-set-discard",
            wrap(Map16DocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}
