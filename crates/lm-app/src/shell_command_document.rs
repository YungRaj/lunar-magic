#[path = "shell_command_document/appearance_documents.rs"]
mod appearance_documents;
#[path = "shell_command_document/file_utilities.rs"]
mod file_utilities;
#[path = "shell_command_document/map16_documents.rs"]
mod map16_documents;
#[path = "shell_command_document/visual_asset_documents.rs"]
mod visual_asset_documents;

pub(super) use appearance_documents::{
    parse_entity_appearance_document_command, parse_overworld_appearance_document_command,
};
pub(super) use file_utilities::{
    parse_copier_header_command, parse_ips_command, parse_standalone_render_command,
};
pub(super) use map16_documents::{parse_map16_document_command, parse_map16_page_document_command};
pub(super) use visual_asset_documents::{
    parse_exanimation_document_command, parse_graphics_document_command,
    parse_palette_document_command,
};

use super::{
    CompleteLevelDocumentCommand, ExpandedSettingsDocumentCommand, Layer3DocumentCommand,
    MetadataDocumentCommand, MwlDocumentCommand, NativeAssetsDocumentCommand,
    NativeLevelDocumentCommand, OverworldDocumentCommand, PathDocumentCommand, ShellCommand,
    ShellCommandError, no_argument, path_argument,
};

pub(super) fn parse_native_assets_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::NativeAssetsDocument(command);
    let result = match command {
        "native-assets-open-file" => path_argument(argument, "native-assets-open-file")
            .map(NativeAssetsDocumentCommand::Open)
            .map(wrap),
        "native-assets-edit-file" => path_argument(argument, "native-assets-edit-file")
            .map(NativeAssetsDocumentCommand::Edit)
            .map(wrap),
        "native-assets-render-file" => path_argument(argument, "native-assets-render-file")
            .map(NativeAssetsDocumentCommand::Render)
            .map(wrap),
        "native-assets-undo" => no_argument(
            argument,
            "native-assets-undo",
            wrap(NativeAssetsDocumentCommand::Undo),
        ),
        "native-assets-redo" => no_argument(
            argument,
            "native-assets-redo",
            wrap(NativeAssetsDocumentCommand::Redo),
        ),
        "native-assets-status" => no_argument(
            argument,
            "native-assets-status",
            wrap(NativeAssetsDocumentCommand::Status),
        ),
        "native-assets-save" => no_argument(
            argument,
            "native-assets-save",
            wrap(NativeAssetsDocumentCommand::Save),
        ),
        "native-assets-close" => no_argument(
            argument,
            "native-assets-close",
            wrap(NativeAssetsDocumentCommand::Close),
        ),
        "native-assets-discard" => no_argument(
            argument,
            "native-assets-discard",
            wrap(NativeAssetsDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(super) fn parse_native_level_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::NativeLevelDocument(command);
    let result = match command {
        "native-level-open" => path_argument(argument, "native-level-open")
            .map(NativeLevelDocumentCommand::Open)
            .map(wrap),
        "native-level-edit-file" => path_argument(argument, "native-level-edit-file")
            .map(NativeLevelDocumentCommand::Edit)
            .map(wrap),
        "native-level-undo" => no_argument(
            argument,
            "native-level-undo",
            wrap(NativeLevelDocumentCommand::Undo),
        ),
        "native-level-redo" => no_argument(
            argument,
            "native-level-redo",
            wrap(NativeLevelDocumentCommand::Redo),
        ),
        "native-level-status" => no_argument(
            argument,
            "native-level-status",
            wrap(NativeLevelDocumentCommand::Status),
        ),
        "native-level-save" => no_argument(
            argument,
            "native-level-save",
            wrap(NativeLevelDocumentCommand::Save),
        ),
        "native-level-close" => no_argument(
            argument,
            "native-level-close",
            wrap(NativeLevelDocumentCommand::Close),
        ),
        "native-level-discard" => no_argument(
            argument,
            "native-level-discard",
            wrap(NativeLevelDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(super) fn parse_mwl_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::MwlDocument(command);
    let result = match command {
        "mwl-open" => path_argument(argument, "mwl-open")
            .map(MwlDocumentCommand::Open)
            .map(wrap),
        "mwl-edit-file" => path_argument(argument, "mwl-edit-file")
            .map(MwlDocumentCommand::Edit)
            .map(wrap),
        "mwl-import-optional-assets-file" => {
            path_argument(argument, "mwl-import-optional-assets-file")
                .map(MwlDocumentCommand::ImportOptionalAssets)
                .map(wrap)
        }
        "mwl-edit-optional-assets-file" => path_argument(argument, "mwl-edit-optional-assets-file")
            .map(MwlDocumentCommand::EditOptionalAssets)
            .map(wrap),
        "mwl-edit-layer3-settings-file" => path_argument(argument, "mwl-edit-layer3-settings-file")
            .map(MwlDocumentCommand::EditLayer3Settings)
            .map(wrap),
        "mwl-undo" => no_argument(argument, "mwl-undo", wrap(MwlDocumentCommand::Undo)),
        "mwl-redo" => no_argument(argument, "mwl-redo", wrap(MwlDocumentCommand::Redo)),
        "mwl-status" => no_argument(argument, "mwl-status", wrap(MwlDocumentCommand::Status)),
        "mwl-save" => no_argument(argument, "mwl-save", wrap(MwlDocumentCommand::Save)),
        "mwl-close" => no_argument(argument, "mwl-close", wrap(MwlDocumentCommand::Close)),
        "mwl-discard" => no_argument(argument, "mwl-discard", wrap(MwlDocumentCommand::Discard)),
        _ => return None,
    };
    Some(result)
}

pub(super) fn parse_overworld_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::OverworldDocument(command);
    let result = match command {
        "world-open-file" => path_argument(argument, "world-open-file")
            .map(OverworldDocumentCommand::Open)
            .map(wrap),
        "world-edit-file" => path_argument(argument, "world-edit-file")
            .map(OverworldDocumentCommand::Edit)
            .map(wrap),
        "world-render-file" => path_argument(argument, "world-render-file")
            .map(OverworldDocumentCommand::Render)
            .map(wrap),
        "world-undo" => no_argument(argument, "world-undo", wrap(OverworldDocumentCommand::Undo)),
        "world-redo" => no_argument(argument, "world-redo", wrap(OverworldDocumentCommand::Redo)),
        "world-status" => no_argument(
            argument,
            "world-status",
            wrap(OverworldDocumentCommand::Status),
        ),
        "world-save" => no_argument(argument, "world-save", wrap(OverworldDocumentCommand::Save)),
        "world-close" => no_argument(
            argument,
            "world-close",
            wrap(OverworldDocumentCommand::Close),
        ),
        "world-discard" => no_argument(
            argument,
            "world-discard",
            wrap(OverworldDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(super) fn parse_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    if let Some(result) = parse_metadata_document_command(command, argument) {
        return Some(result);
    }
    if let Some(result) = parse_path_document_command(command, argument) {
        return Some(result);
    }
    let result = match command {
        "layer3-open" => path_argument(argument, "layer3-open")
            .map(|path| ShellCommand::Layer3Document(Layer3DocumentCommand::Open(path))),
        "layer3-edit-file" => path_argument(argument, "layer3-edit-file")
            .map(|path| ShellCommand::Layer3Document(Layer3DocumentCommand::Edit(path))),
        "layer3-undo" => no_argument(
            argument,
            "layer3-undo",
            ShellCommand::Layer3Document(Layer3DocumentCommand::Undo),
        ),
        "layer3-redo" => no_argument(
            argument,
            "layer3-redo",
            ShellCommand::Layer3Document(Layer3DocumentCommand::Redo),
        ),
        "layer3-status" => no_argument(
            argument,
            "layer3-status",
            ShellCommand::Layer3Document(Layer3DocumentCommand::Status),
        ),
        "layer3-save" => no_argument(
            argument,
            "layer3-save",
            ShellCommand::Layer3Document(Layer3DocumentCommand::Save),
        ),
        "layer3-close" => no_argument(
            argument,
            "layer3-close",
            ShellCommand::Layer3Document(Layer3DocumentCommand::Close),
        ),
        "layer3-discard" => no_argument(
            argument,
            "layer3-discard",
            ShellCommand::Layer3Document(Layer3DocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

fn parse_metadata_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = ShellCommand::MetadataDocument;
    let result = match command {
        "metadata-open" => path_argument(argument, "metadata-open")
            .map(MetadataDocumentCommand::Open)
            .map(wrap),
        "metadata-edit" => path_argument(argument, "metadata-edit")
            .map(MetadataDocumentCommand::Edit)
            .map(wrap),
        "metadata-undo" => no_argument(
            argument,
            "metadata-undo",
            wrap(MetadataDocumentCommand::Undo),
        ),
        "metadata-redo" => no_argument(
            argument,
            "metadata-redo",
            wrap(MetadataDocumentCommand::Redo),
        ),
        "metadata-status" => no_argument(
            argument,
            "metadata-status",
            wrap(MetadataDocumentCommand::Status),
        ),
        "metadata-save" => no_argument(
            argument,
            "metadata-save",
            wrap(MetadataDocumentCommand::Save),
        ),
        "metadata-close" => no_argument(
            argument,
            "metadata-close",
            wrap(MetadataDocumentCommand::Close),
        ),
        "metadata-discard" => no_argument(
            argument,
            "metadata-discard",
            wrap(MetadataDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

fn parse_path_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = ShellCommand::PathDocument;
    let result = match command {
        "path-open" => path_argument(argument, "path-open")
            .map(PathDocumentCommand::Open)
            .map(wrap),
        "path-edit" => path_argument(argument, "path-edit")
            .map(PathDocumentCommand::Edit)
            .map(wrap),
        "path-undo" => no_argument(argument, "path-undo", wrap(PathDocumentCommand::Undo)),
        "path-redo" => no_argument(argument, "path-redo", wrap(PathDocumentCommand::Redo)),
        "path-status" => no_argument(argument, "path-status", wrap(PathDocumentCommand::Status)),
        "path-save" => no_argument(argument, "path-save", wrap(PathDocumentCommand::Save)),
        "path-close" => no_argument(argument, "path-close", wrap(PathDocumentCommand::Close)),
        "path-discard" => no_argument(argument, "path-discard", wrap(PathDocumentCommand::Discard)),
        _ => return None,
    };
    Some(result)
}

pub(super) fn parse_expanded_settings_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = ShellCommand::ExpandedSettingsDocument;
    let result = match command {
        "expanded-settings-open" => path_argument(argument, "expanded-settings-open")
            .map(ExpandedSettingsDocumentCommand::Open)
            .map(wrap),
        "expanded-settings-edit-file" => path_argument(argument, "expanded-settings-edit-file")
            .map(ExpandedSettingsDocumentCommand::Edit)
            .map(wrap),
        "expanded-settings-undo" => no_argument(
            argument,
            "expanded-settings-undo",
            wrap(ExpandedSettingsDocumentCommand::Undo),
        ),
        "expanded-settings-redo" => no_argument(
            argument,
            "expanded-settings-redo",
            wrap(ExpandedSettingsDocumentCommand::Redo),
        ),
        "expanded-settings-status" => no_argument(
            argument,
            "expanded-settings-status",
            wrap(ExpandedSettingsDocumentCommand::Status),
        ),
        "expanded-settings-save" => no_argument(
            argument,
            "expanded-settings-save",
            wrap(ExpandedSettingsDocumentCommand::Save),
        ),
        "expanded-settings-close" => no_argument(
            argument,
            "expanded-settings-close",
            wrap(ExpandedSettingsDocumentCommand::Close),
        ),
        "expanded-settings-discard" => no_argument(
            argument,
            "expanded-settings-discard",
            wrap(ExpandedSettingsDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}

pub(super) fn parse_complete_level_document_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    let wrap = |command| ShellCommand::CompleteLevelDocument(command);
    let result = match command {
        "bundle-open" => path_argument(argument, "bundle-open")
            .map(CompleteLevelDocumentCommand::Open)
            .map(wrap),
        "bundle-edit-file" => path_argument(argument, "bundle-edit-file")
            .map(CompleteLevelDocumentCommand::Edit)
            .map(wrap),
        "bundle-render-file" => path_argument(argument, "bundle-render-file")
            .map(CompleteLevelDocumentCommand::Render)
            .map(wrap),
        "bundle-undo" => no_argument(
            argument,
            "bundle-undo",
            wrap(CompleteLevelDocumentCommand::Undo),
        ),
        "bundle-redo" => no_argument(
            argument,
            "bundle-redo",
            wrap(CompleteLevelDocumentCommand::Redo),
        ),
        "bundle-status" => no_argument(
            argument,
            "bundle-status",
            wrap(CompleteLevelDocumentCommand::Status),
        ),
        "bundle-save" => no_argument(
            argument,
            "bundle-save",
            wrap(CompleteLevelDocumentCommand::Save),
        ),
        "bundle-close" => no_argument(
            argument,
            "bundle-close",
            wrap(CompleteLevelDocumentCommand::Close),
        ),
        "bundle-discard" => no_argument(
            argument,
            "bundle-discard",
            wrap(CompleteLevelDocumentCommand::Discard),
        ),
        _ => return None,
    };
    Some(result)
}
