use super::super::{
    ExAnimationDocumentCommand, GraphicsDocumentCommand, PaletteDocumentCommand, ShellCommand,
    ShellCommandError, no_argument, path_argument,
};

pub(in super::super) fn parse_exanimation_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::ExAnimationDocument(command);
    let result = match command {
        "ex-open-file" => path_argument(argument, "ex-open-file")
            .map(ExAnimationDocumentCommand::Open)
            .map(wrap),
        "ex-edit-file" => path_argument(argument, "ex-edit-file")
            .map(ExAnimationDocumentCommand::Edit)
            .map(wrap),
        "ex-undo" => no_argument(argument, "ex-undo", wrap(ExAnimationDocumentCommand::Undo)),
        "ex-redo" => no_argument(argument, "ex-redo", wrap(ExAnimationDocumentCommand::Redo)),
        "ex-status" => no_argument(
            argument,
            "ex-status",
            wrap(ExAnimationDocumentCommand::Status),
        ),
        "ex-save" => no_argument(argument, "ex-save", wrap(ExAnimationDocumentCommand::Save)),
        "ex-close" => no_argument(
            argument,
            "ex-close",
            wrap(ExAnimationDocumentCommand::Close),
        ),
        "ex-discard" => no_argument(
            argument,
            "ex-discard",
            wrap(ExAnimationDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(in super::super) fn parse_palette_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::PaletteDocument(command);
    let result = match command {
        "pal-open" => path_argument(argument, "pal-open")
            .map(PaletteDocumentCommand::Open)
            .map(wrap),
        "pal-edit-file" => path_argument(argument, "pal-edit-file")
            .map(PaletteDocumentCommand::Edit)
            .map(wrap),
        "pal-render-file" => path_argument(argument, "pal-render-file")
            .map(PaletteDocumentCommand::Render)
            .map(wrap),
        "pal-undo" => no_argument(argument, "pal-undo", wrap(PaletteDocumentCommand::Undo)),
        "pal-redo" => no_argument(argument, "pal-redo", wrap(PaletteDocumentCommand::Redo)),
        "pal-status" => no_argument(argument, "pal-status", wrap(PaletteDocumentCommand::Status)),
        "pal-save" => no_argument(argument, "pal-save", wrap(PaletteDocumentCommand::Save)),
        "pal-close" => no_argument(argument, "pal-close", wrap(PaletteDocumentCommand::Close)),
        "pal-discard" => no_argument(
            argument,
            "pal-discard",
            wrap(PaletteDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(in super::super) fn parse_graphics_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::GraphicsDocument(command);
    let result = match command {
        "gfx-open" => path_argument(argument, "gfx-open")
            .map(GraphicsDocumentCommand::Open)
            .map(wrap),
        "gfx-edit-file" => path_argument(argument, "gfx-edit-file")
            .map(GraphicsDocumentCommand::Edit)
            .map(wrap),
        "gfx-render-file" => path_argument(argument, "gfx-render-file")
            .map(GraphicsDocumentCommand::Render)
            .map(wrap),
        "gfx-undo" => no_argument(argument, "gfx-undo", wrap(GraphicsDocumentCommand::Undo)),
        "gfx-redo" => no_argument(argument, "gfx-redo", wrap(GraphicsDocumentCommand::Redo)),
        "gfx-status" => no_argument(
            argument,
            "gfx-status",
            wrap(GraphicsDocumentCommand::Status),
        ),
        "gfx-save" => no_argument(argument, "gfx-save", wrap(GraphicsDocumentCommand::Save)),
        "gfx-close" => no_argument(argument, "gfx-close", wrap(GraphicsDocumentCommand::Close)),
        "gfx-discard" => no_argument(
            argument,
            "gfx-discard",
            wrap(GraphicsDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}
