use lm_project::GraphicsCompression;
use std::fmt;
use std::path::PathBuf;

#[path = "shell_command_arguments.rs"]
mod arguments;

use arguments::{
    decimal_argument, hex_argument, hex_usize, hex_value, no_argument, path_argument,
    single_string_argument,
};

pub use crate::shell_document_command::{
    CompleteLevelDocumentCommand, EntityAppearanceDocumentCommand, ExAnimationDocumentCommand,
    ExpandedSettingsDocumentCommand, GraphicsDocumentCommand, Layer3DocumentCommand,
    Map16DocumentCommand, Map16PageDocumentCommand, MetadataDocumentCommand, MwlDocumentCommand,
    NativeAssetsDocumentCommand, NativeLevelDocumentCommand, OverworldAppearanceDocumentCommand,
    OverworldDocumentCommand, PaletteDocumentCommand, PathDocumentCommand,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelHeaderField {
    BackgroundPalette,
    LevelMode,
    BackgroundColor,
    SpriteTileset,
    DefaultMusicSelector,
    SpritePalette,
    ForegroundPalette,
    ObjectTileset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptEditor {
    NativeAssets,
    ExAnimation,
    Graphics,
    Level,
    Map16,
    Overworld,
    Palette,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiCommand {
    Install(PathBuf),
    Status,
    Action(String),
    Shortcut(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolCommand {
    Install(PathBuf),
    Status,
    Run(String),
    Event(String),
    Execute(String),
    ExecuteEvent(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellCommand {
    Empty,
    Help,
    Status,
    Recent,
    OpenRecent(usize),
    Open(PathBuf),
    Close,
    InstallRevisionProfile(PathBuf),
    InstallRevisionPatch(PathBuf),
    InstallSettings,
    InstallLayer3,
    NativeOverworldPathExport(PathBuf),
    NativeOverworldPathImport(PathBuf),
    NativeOverworldMessageExport(PathBuf),
    NativeOverworldMessageImport(PathBuf),
    NativeOverworldEventExport(PathBuf),
    NativeOverworldEventImport(PathBuf),
    NativeOverworldEventMapExport(PathBuf),
    NativeOverworldEventMapImport(PathBuf),
    NativeOverworldSpecialEventExport(PathBuf),
    NativeOverworldSpecialEventImport(PathBuf),
    NativeOverworldEventTilemapExport(PathBuf),
    NativeOverworldEventTilemapImport(PathBuf),
    NativeOverworldBossSequenceExport(PathBuf),
    NativeOverworldBossSequenceImport(PathBuf),
    NativeCreditsTilemapExport(PathBuf),
    NativeCreditsTilemapImport(PathBuf),
    NativeTitleTilemapExport(PathBuf),
    NativeTitleTilemapImport(PathBuf),
    NativeTitleRecordingExport(PathBuf),
    NativeTitleRecordingImport(PathBuf),
    NativeTitleRecordingZsnesExport(PathBuf),
    NativeTitleRecordingZsnesImport(PathBuf),
    NativeTitleRecordingSnes9xImport(PathBuf),
    LunarMagicMetadataExport(PathBuf),
    LunarMagicMetadataImport(PathBuf),
    NativeSecondaryExitExport(PathBuf),
    NativeSecondaryExitImport(PathBuf),
    NativeOverworldWarpExport(PathBuf),
    NativeOverworldWarpImport(PathBuf),
    NativeOverworldLevelNameExport(PathBuf),
    NativeOverworldLevelNameImport(PathBuf),
    NativeOverworldSettingsExport(PathBuf),
    NativeOverworldSettingsImport(PathBuf),
    NativeOverworldPlayerStartExport(PathBuf),
    NativeOverworldPlayerStartImport(PathBuf),
    ClearRevisionProfile,
    CustomObjectOpen(PathBuf),
    CustomObjectEdit(PathBuf),
    CustomObjectUndo,
    CustomObjectRedo,
    CustomObjectStatus,
    CustomObjectSave,
    CustomObjectClose,
    CustomObjectDiscard,
    CustomSpriteOpen(PathBuf),
    CustomSpriteEdit(PathBuf),
    CustomSpriteUndo,
    CustomSpriteRedo,
    CustomSpriteStatus,
    CustomSpriteSave,
    CustomSpriteClose,
    CustomSpriteDiscard,
    DscSidecarOpen(PathBuf),
    DscSidecarReplace(PathBuf),
    DscSidecarUndo,
    DscSidecarRedo,
    DscSidecarStatus,
    DscSidecarSave,
    DscSidecarClose,
    DscSidecarDiscard,
    NativeMap16SidecarOpen(PathBuf),
    NativeMap16SidecarEdit(PathBuf),
    NativeMap16SidecarUndo,
    NativeMap16SidecarRedo,
    NativeMap16SidecarStatus,
    NativeMap16SidecarSave,
    NativeMap16SidecarClose,
    NativeMap16SidecarDiscard,
    MetadataDocument(MetadataDocumentCommand),
    PathDocument(PathDocumentCommand),
    Layer3Document(Layer3DocumentCommand),
    ExpandedSettingsDocument(ExpandedSettingsDocumentCommand),
    CompleteLevelDocument(CompleteLevelDocumentCommand),
    Map16Document(Map16DocumentCommand),
    Map16PageDocument(Map16PageDocumentCommand),
    OverworldDocument(OverworldDocumentCommand),
    OverworldAppearanceDocument(OverworldAppearanceDocumentCommand),
    GraphicsDocument(GraphicsDocumentCommand),
    PaletteDocument(PaletteDocumentCommand),
    ExAnimationDocument(ExAnimationDocumentCommand),
    EntityAppearanceDocument(EntityAppearanceDocumentCommand),
    MwlDocument(MwlDocumentCommand),
    NativeLevelDocument(NativeLevelDocumentCommand),
    NativeAssetsDocument(NativeAssetsDocumentCommand),
    RenderMap16(PathBuf),
    RenderOverworld(PathBuf),
    IpsApply(PathBuf),
    IpsCreate(PathBuf),
    CopierHeaderAdd(PathBuf),
    CopierHeaderRemove(PathBuf),
    Ui(UiCommand),
    Tool(ToolCommand),
    SelectLevel(u16),
    LevelBack,
    LevelForward,
    SetLevelViewport {
        x: i64,
        y: i64,
        zoom_numerator: u32,
        zoom_denominator: u32,
    },
    EditLevelHeader {
        field: LevelHeaderField,
        value: u8,
        search_start: usize,
        search_end: usize,
    },
    EditExpandedSettingsWord {
        index: usize,
        value: u16,
    },
    EditExpandedSettings(PathBuf),
    ApplyEditorScript {
        editor: ScriptEditor,
        script: PathBuf,
        search_start: usize,
        search_end: usize,
    },
    ApplyOwnedEditorScript {
        editor: ScriptEditor,
        script: PathBuf,
        ownership_manifest: PathBuf,
        search_start: usize,
        search_end: usize,
    },
    /// Imports a complete binary MWL into the currently selected native level.
    ImportMwlLevel {
        path: PathBuf,
        search_start: usize,
        search_end: usize,
    },
    /// Imports every visible MWL in a directory into the level declared by that file.
    ImportMwlLevelDirectory {
        path: PathBuf,
        search_start: usize,
        search_end: usize,
    },
    /// Exports the currently selected installed native level as a complete binary MWL.
    ExportMwlLevel(PathBuf),
    /// Exports every profile-addressable native level using Lunar Magic's numbered naming rule.
    ExportAllMwlLevels(PathBuf),
    /// Exports only levels whose Layer 1 payload is outside pristine SMW ROM data.
    ExportModifiedMwlLevels(PathBuf),
    MigrateGraphicsCompression {
        target: GraphicsCompression,
        search_start: usize,
        search_end: usize,
    },
    ExpandRom {
        target_logical_len: usize,
        fill: u8,
    },
    ShowOverworld,
    ShowMap16,
    ShowGraphics(u16),
    ShowPalette(u16),
    ShowExAnimation(u16),
    ShowLayer3(u16),
    Undo,
    Redo,
    Save,
    SaveAs(PathBuf),
    Quit,
    Unknown(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellCommandError {
    MissingArgument(&'static str),
    UnexpectedArgument(&'static str),
    InvalidHex {
        command: &'static str,
        value: String,
    },
    InvalidIndex {
        command: &'static str,
        value: String,
    },
    InvalidLevelHeaderField(String),
    InvalidGraphicsCompression(String),
    InvalidRange {
        command: &'static str,
        value: String,
    },
}

impl fmt::Display for ShellCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid shell command: {self:?}")
    }
}

impl std::error::Error for ShellCommandError {}

/// Parses one command line while preserving spaces and Unicode in path arguments.
#[allow(clippy::too_many_lines)] // Exhaustive command router; domain parsers remain focused.
pub fn parse(line: &str) -> Result<ShellCommand, ShellCommandError> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(ShellCommand::Empty);
    }
    let (command, argument) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(command, argument)| (command, argument.trim()));
    if let Some(document) = parse_portable_command(command, argument) {
        return document;
    }
    if let Some(title) = parse_title_command(command, argument) {
        return title;
    }
    if let Some(profile) = parse_profile_command(command, argument) {
        return profile;
    }
    match command {
        "help" => no_argument(argument, "help", ShellCommand::Help),
        "status" => no_argument(argument, "status", ShellCommand::Status),
        "recent" => no_argument(argument, "recent", ShellCommand::Recent),
        "open-recent" => Ok(ShellCommand::OpenRecent(decimal_argument(
            argument,
            "open-recent",
        )?)),
        "open" => Ok(ShellCommand::Open(path_argument(argument, "open")?)),
        "close" => no_argument(argument, "close", ShellCommand::Close),
        "ui-config" => Ok(ShellCommand::Ui(UiCommand::Install(path_argument(
            argument,
            "ui-config",
        )?))),
        "ui-status" => no_argument(argument, "ui-status", ShellCommand::Ui(UiCommand::Status)),
        "ui-action" => Ok(ShellCommand::Ui(UiCommand::Action(single_string_argument(
            argument,
            "ui-action",
        )?))),
        "ui-shortcut" => Ok(ShellCommand::Ui(UiCommand::Shortcut(
            single_string_argument(argument, "ui-shortcut")?,
        ))),
        "tools-config" => Ok(ShellCommand::Tool(ToolCommand::Install(path_argument(
            argument,
            "tools-config",
        )?))),
        "tools-status" => no_argument(
            argument,
            "tools-status",
            ShellCommand::Tool(ToolCommand::Status),
        ),
        "tool-run" => Ok(ShellCommand::Tool(ToolCommand::Run(
            single_string_argument(argument, "tool-run")?,
        ))),
        "tool-event" => Ok(ShellCommand::Tool(ToolCommand::Event(
            single_string_argument(argument, "tool-event")?,
        ))),
        "tool-exec" => Ok(ShellCommand::Tool(ToolCommand::Execute(
            single_string_argument(argument, "tool-exec")?,
        ))),
        "tool-event-exec" => Ok(ShellCommand::Tool(ToolCommand::ExecuteEvent(
            single_string_argument(argument, "tool-event-exec")?,
        ))),
        "level" => Ok(ShellCommand::SelectLevel(hex_argument(argument, "level")?)),
        "level-back" => no_argument(argument, "level-back", ShellCommand::LevelBack),
        "level-forward" => no_argument(argument, "level-forward", ShellCommand::LevelForward),
        "level-view" => parse_level_view(argument),
        "level-header" => parse_level_header_edit(argument),
        "expanded-settings-word" => parse_expanded_settings_word(argument),
        "expanded-settings-edit" => Ok(ShellCommand::EditExpandedSettings(path_argument(
            argument,
            "expanded-settings-edit",
        )?)),
        "level-edit" => parse_level_edit_script(argument),
        "native-assets-edit" => parse_native_assets_edit_script(argument),
        "level-mwl-import"
        | "level-mwl-import-dir"
        | "level-mwl-export"
        | "level-mwl-export-all"
        | "level-mwl-export-modified" => parse_mwl_level_command(command, argument),
        "overworld" => no_argument(argument, "overworld", ShellCommand::ShowOverworld),
        "overworld-edit" => parse_overworld_edit_script(argument),
        "map16" => no_argument(argument, "map16", ShellCommand::ShowMap16),
        "map16-edit" => parse_map16_edit_script(argument),
        "graphics" => Ok(ShellCommand::ShowGraphics(hex_argument(
            argument, "graphics",
        )?)),
        "graphics-edit" => parse_graphics_edit_script(argument),
        "graphics-recompress" => parse_graphics_recompression(argument),
        "rom-expand" => parse_rom_expansion(argument),
        "palette" => Ok(ShellCommand::ShowPalette(hex_argument(
            argument, "palette",
        )?)),
        "palette-edit" => parse_palette_edit_script(argument),
        "exanimation" => Ok(ShellCommand::ShowExAnimation(hex_argument(
            argument,
            "exanimation",
        )?)),
        "exanimation-edit" => parse_exanimation_edit_script(argument),
        "layer3" => Ok(ShellCommand::ShowLayer3(hex_argument(argument, "layer3")?)),
        "undo" => no_argument(argument, "undo", ShellCommand::Undo),
        "redo" => no_argument(argument, "redo", ShellCommand::Redo),
        "save" => no_argument(argument, "save", ShellCommand::Save),
        "save-as" => Ok(ShellCommand::SaveAs(path_argument(argument, "save-as")?)),
        "quit" | "exit" => no_argument(argument, "quit", ShellCommand::Quit),
        unknown => Ok(ShellCommand::Unknown(unknown.to_owned())),
    }
}

fn parse_profile_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    Some(match command {
        "profile" => path_argument(argument, "profile").map(ShellCommand::InstallRevisionProfile),
        "profile-clear" => no_argument(
            argument,
            "profile-clear",
            ShellCommand::ClearRevisionProfile,
        ),
        "revision-patch-install-file" => path_argument(argument, "revision-patch-install-file")
            .map(ShellCommand::InstallRevisionPatch),
        "expanded-settings-install" => no_argument(
            argument,
            "expanded-settings-install",
            ShellCommand::InstallSettings,
        ),
        "layer3-install" => no_argument(argument, "layer3-install", ShellCommand::InstallLayer3),
        "overworld-native-path-export" => path_argument(argument, "overworld-native-path-export")
            .map(ShellCommand::NativeOverworldPathExport),
        "overworld-native-path-import" => path_argument(argument, "overworld-native-path-import")
            .map(ShellCommand::NativeOverworldPathImport),
        "overworld-native-message-export" => {
            path_argument(argument, "overworld-native-message-export")
                .map(ShellCommand::NativeOverworldMessageExport)
        }
        "overworld-native-message-import" => {
            path_argument(argument, "overworld-native-message-import")
                .map(ShellCommand::NativeOverworldMessageImport)
        }
        "overworld-native-event-export" => path_argument(argument, "overworld-native-event-export")
            .map(ShellCommand::NativeOverworldEventExport),
        "overworld-native-event-import" => path_argument(argument, "overworld-native-event-import")
            .map(ShellCommand::NativeOverworldEventImport),
        "overworld-native-event-map-export" => {
            path_argument(argument, "overworld-native-event-map-export")
                .map(ShellCommand::NativeOverworldEventMapExport)
        }
        "overworld-native-event-map-import" => {
            path_argument(argument, "overworld-native-event-map-import")
                .map(ShellCommand::NativeOverworldEventMapImport)
        }
        "overworld-native-special-event-export" => {
            path_argument(argument, "overworld-native-special-event-export")
                .map(ShellCommand::NativeOverworldSpecialEventExport)
        }
        "overworld-native-special-event-import" => {
            path_argument(argument, "overworld-native-special-event-import")
                .map(ShellCommand::NativeOverworldSpecialEventImport)
        }
        "overworld-native-event-tilemap-export" => {
            path_argument(argument, "overworld-native-event-tilemap-export")
                .map(ShellCommand::NativeOverworldEventTilemapExport)
        }
        "overworld-native-event-tilemap-import" => {
            path_argument(argument, "overworld-native-event-tilemap-import")
                .map(ShellCommand::NativeOverworldEventTilemapImport)
        }
        "overworld-native-boss-sequence-export" => {
            path_argument(argument, "overworld-native-boss-sequence-export")
                .map(ShellCommand::NativeOverworldBossSequenceExport)
        }
        "overworld-native-boss-sequence-import" => {
            path_argument(argument, "overworld-native-boss-sequence-import")
                .map(ShellCommand::NativeOverworldBossSequenceImport)
        }
        "credits-native-tilemap-export" => path_argument(argument, "credits-native-tilemap-export")
            .map(ShellCommand::NativeCreditsTilemapExport),
        "credits-native-tilemap-import" => path_argument(argument, "credits-native-tilemap-import")
            .map(ShellCommand::NativeCreditsTilemapImport),
        "overworld-native-warp-export" => path_argument(argument, "overworld-native-warp-export")
            .map(ShellCommand::NativeOverworldWarpExport),
        "overworld-native-warp-import" => path_argument(argument, "overworld-native-warp-import")
            .map(ShellCommand::NativeOverworldWarpImport),
        "overworld-native-name-export" => path_argument(argument, "overworld-native-name-export")
            .map(ShellCommand::NativeOverworldLevelNameExport),
        "overworld-native-name-import" => path_argument(argument, "overworld-native-name-import")
            .map(ShellCommand::NativeOverworldLevelNameImport),
        "overworld-native-settings-export" => {
            path_argument(argument, "overworld-native-settings-export")
                .map(ShellCommand::NativeOverworldSettingsExport)
        }
        "overworld-native-settings-import" => {
            path_argument(argument, "overworld-native-settings-import")
                .map(ShellCommand::NativeOverworldSettingsImport)
        }
        "overworld-native-start-export" => path_argument(argument, "overworld-native-start-export")
            .map(ShellCommand::NativeOverworldPlayerStartExport),
        "overworld-native-start-import" => path_argument(argument, "overworld-native-start-import")
            .map(ShellCommand::NativeOverworldPlayerStartImport),
        _ => return None,
    })
}

fn parse_title_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    Some(match command {
        "title-native-tilemap-export" => path_argument(argument, "title-native-tilemap-export")
            .map(ShellCommand::NativeTitleTilemapExport),
        "title-native-tilemap-import" => path_argument(argument, "title-native-tilemap-import")
            .map(ShellCommand::NativeTitleTilemapImport),
        "title-native-recording-export" => path_argument(argument, "title-native-recording-export")
            .map(ShellCommand::NativeTitleRecordingExport),
        "title-native-recording-import" => path_argument(argument, "title-native-recording-import")
            .map(ShellCommand::NativeTitleRecordingImport),
        "title-recording-zst-export" => path_argument(argument, "title-recording-zst-export")
            .map(ShellCommand::NativeTitleRecordingZsnesExport),
        "title-recording-zst-import" => path_argument(argument, "title-recording-zst-import")
            .map(ShellCommand::NativeTitleRecordingZsnesImport),
        "title-recording-s9x-import" => path_argument(argument, "title-recording-s9x-import")
            .map(ShellCommand::NativeTitleRecordingSnes9xImport),
        "lm-metadata-export" => path_argument(argument, "lm-metadata-export")
            .map(ShellCommand::LunarMagicMetadataExport),
        "lm-metadata-import" => path_argument(argument, "lm-metadata-import")
            .map(ShellCommand::LunarMagicMetadataImport),
        "secondary-exit-native-export" => path_argument(argument, "secondary-exit-native-export")
            .map(ShellCommand::NativeSecondaryExitExport),
        "secondary-exit-native-import" => path_argument(argument, "secondary-exit-native-import")
            .map(ShellCommand::NativeSecondaryExitImport),
        _ => return None,
    })
}

fn parse_portable_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    parse_copier_header_command(command, argument)
        .or_else(|| parse_owned_editor_command(command, argument))
        .or_else(|| parse_custom_library_command(command, argument))
        .or_else(|| parse_ips_command(command, argument))
        .or_else(|| parse_complete_level_document_command(command, argument))
        .or_else(|| parse_map16_document_command(command, argument))
        .or_else(|| parse_map16_page_document_command(command, argument))
        .or_else(|| parse_overworld_document_command(command, argument))
        .or_else(|| parse_overworld_appearance_document_command(command, argument))
        .or_else(|| parse_graphics_document_command(command, argument))
        .or_else(|| parse_palette_document_command(command, argument))
        .or_else(|| parse_exanimation_document_command(command, argument))
        .or_else(|| parse_entity_appearance_document_command(command, argument))
        .or_else(|| parse_mwl_document_command(command, argument))
        .or_else(|| parse_native_level_document_command(command, argument))
        .or_else(|| parse_native_assets_document_command(command, argument))
        .or_else(|| parse_expanded_settings_document_command(command, argument))
        .or_else(|| parse_document_command(command, argument))
        .or_else(|| parse_standalone_render_command(command, argument))
}

#[path = "shell_command_document.rs"]
mod document;

use document::{
    parse_complete_level_document_command, parse_copier_header_command, parse_document_command,
    parse_entity_appearance_document_command, parse_exanimation_document_command,
    parse_expanded_settings_document_command, parse_graphics_document_command, parse_ips_command,
    parse_map16_document_command, parse_map16_page_document_command, parse_mwl_document_command,
    parse_native_assets_document_command, parse_native_level_document_command,
    parse_overworld_appearance_document_command, parse_overworld_document_command,
    parse_palette_document_command, parse_standalone_render_command,
};

#[path = "shell_command_custom_libraries.rs"]
mod custom_libraries;

use custom_libraries::parse_custom_library_command;

#[path = "shell_command_editor.rs"]
mod editor;

use editor::{
    parse_exanimation_edit_script, parse_expanded_settings_word, parse_graphics_edit_script,
    parse_graphics_recompression, parse_level_edit_script, parse_level_header_edit,
    parse_map16_edit_script, parse_mwl_level_command, parse_native_assets_edit_script,
    parse_overworld_edit_script, parse_owned_editor_command, parse_palette_edit_script,
    parse_rom_expansion,
};

fn parse_level_view(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    const COMMAND: &str = "level-view";
    let values = argument.split_whitespace().collect::<Vec<_>>();
    if values.is_empty() {
        return Err(ShellCommandError::MissingArgument(COMMAND));
    }
    if values.len() != 4 {
        return Err(ShellCommandError::UnexpectedArgument(COMMAND));
    }
    let invalid = |value: &str| ShellCommandError::InvalidRange {
        command: COMMAND,
        value: value.into(),
    };
    Ok(ShellCommand::SetLevelViewport {
        x: values[0].parse().map_err(|_| invalid(values[0]))?,
        y: values[1].parse().map_err(|_| invalid(values[1]))?,
        zoom_numerator: values[2].parse().map_err(|_| invalid(values[2]))?,
        zoom_denominator: values[3].parse().map_err(|_| invalid(values[3]))?,
    })
}

#[cfg(test)]
#[path = "shell_command_tests.rs"]
mod tests;
