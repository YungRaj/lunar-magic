//! Toolkit-independent application shell for native frontends.

mod app_types;
mod clipboard;
mod command;
mod complete_level_document_controller;
mod copier_header_state;
mod credits_tilemap_state;
mod custom_object_controller;
mod custom_sprite_controller;
mod document;
mod dsc_sidecar_controller;
mod entity_appearance_document_controller;
mod exanimation_clipboard;
mod exanimation_controller;
mod exanimation_document_controller;
mod exanimation_slot_options_controller;
mod expanded_settings_controller;
mod expanded_settings_document_controller;
mod external_tools;
pub mod file_persistence;
mod frontend_config;
mod frontend_state;
mod graphics_controller;
mod graphics_document_controller;
mod graphics_edit_batch;
mod graphics_migration_state;
mod graphics_ownership_file;
mod ips_patch_state;
mod layer3_document_controller;
mod level_controller;
mod level_navigation;
mod localization;
mod lunar_magic_metadata_state;
mod map16_controller;
mod map16_document_controller;
mod map16_page_document_controller;
mod mwl_document_controller;
mod native_level_assets_controller;
mod native_level_assets_document_controller;
mod native_level_document_controller;
mod native_level_edit_batch;
mod native_map16_sidecar_controller;
mod navigation_state;
mod osc_sidecar_controller;
mod overworld_appearance_document_controller;
mod overworld_boss_sequence_state;
mod overworld_controller;
mod overworld_document_controller;
mod overworld_edit_batch;
mod overworld_event_number_state;
mod overworld_event_state;
mod overworld_event_tilemap_state;
mod overworld_level_name_state;
mod overworld_message_state;
mod overworld_metadata_controller;
mod overworld_path_controller;
mod overworld_path_link_state;
mod overworld_player_start_state;
mod overworld_settings_state;
mod overworld_special_event_state;
mod overworld_warp_link_state;
mod palette_controller;
mod palette_document_controller;
mod palette_edit_batch;
mod palette_ownership_file;
mod persistence;
mod portable_value_history;
mod prepared_commit;
mod profile_controller;
mod project_state;
mod rats_reclamation_state;
mod recent_documents;
pub mod recent_state_file;
mod revision_patch_state;
mod revision_profile;
mod revision_profile_state;
mod rom_expansion_state;
mod secondary_exit_state;
mod selection;
mod selection_state;
mod shared_palette_state;
mod shortcut;
mod smw_map16_controller;
mod snapshot;
mod ssc_sidecar_controller;
pub mod startup_args;
mod state;
mod title_recording_state;
mod title_tilemap_state;
mod tool_config;
mod tool_state;
mod toolbar;
mod vanilla_entrance_controller;
mod viewport_rendering;

pub use app_types::{
    AppCapabilities, EditorMode, FrontendEffect, HistoryCapabilities, NavigationCapabilities,
    ProfileStatus, ProjectStatus, SaveStatus, SelectionCapabilities,
};
pub use clipboard::{ClipboardError, ClipboardKind, ClipboardPayload};
pub use command::{Command, RomExpansionCommand};
pub use complete_level_document_controller::{
    CompleteLevelDocumentController, CompleteLevelDocumentControllerError,
    CompleteLevelDocumentEdit, CompleteLevelDocumentEditError, CompleteLevelDocumentSaveSnapshot,
};
pub use custom_object_controller::{
    CustomObjectControllerError, CustomObjectLibraryController, CustomObjectLibraryEdit,
    CustomObjectSaveSnapshot,
};
pub use custom_sprite_controller::{
    CustomSpriteControllerError, CustomSpriteLibraryController, CustomSpriteLibraryEdit,
    CustomSpriteSaveSnapshot,
};
pub use document::PreparedRomOpen;
pub use dsc_sidecar_controller::{
    DscSidecarController, DscSidecarControllerError, DscSidecarSaveSnapshot,
};
pub use entity_appearance_document_controller::{
    EntityAppearanceDocumentController, EntityAppearanceDocumentControllerError,
    EntityAppearanceDocumentEdit, EntityAppearanceDocumentSaveSnapshot,
};
pub use exanimation_clipboard::{
    ExAnimationClipboardError, copy_exanimation_frames, cut_exanimation_frames,
    paste_exanimation_frames,
};
pub use exanimation_controller::{
    ExAnimationController, ExAnimationControllerEdit, ExAnimationControllerEditFailure,
    ExAnimationControllerError,
};
pub use exanimation_document_controller::{
    ExAnimationDocumentController, ExAnimationDocumentControllerError,
    ExAnimationDocumentSaveSnapshot,
};
pub use exanimation_slot_options_controller::{
    ExAnimationSlotOptionEdit, ExAnimationSlotOptionsController,
    ExAnimationSlotOptionsControllerError,
};
pub use expanded_settings_controller::{
    ExpandedSettingsController, ExpandedSettingsControllerError,
};
pub use expanded_settings_document_controller::{
    ExpandedSettingsDocumentController, ExpandedSettingsDocumentControllerError,
    ExpandedSettingsDocumentSaveSnapshot,
};
pub use external_tools::{
    ExternalTool, ExternalToolError, ToolContext, ToolEvent, ToolInvocation, validate_tools,
};
pub use frontend_config::{FrontendConfig, FrontendConfigError};
pub use graphics_controller::{
    GraphicsController, GraphicsControllerEdit, GraphicsControllerError,
};
pub use graphics_document_controller::{
    GraphicsDocumentController, GraphicsDocumentControllerError, GraphicsDocumentSaveSnapshot,
};
pub use graphics_ownership_file::{GraphicsOwnershipFile, GraphicsOwnershipFileError};
pub use layer3_document_controller::{
    Layer3DocumentController, Layer3DocumentControllerError, Layer3DocumentSaveSnapshot,
};
pub use level_controller::{LevelController, LevelControllerError, NativeLevelEdit};
pub use level_navigation::{
    LevelNavigationDirection, LevelViewState, LevelViewport, LevelViewportError,
};
pub use lm_project::{MwlOptionalAssetsEdit, MwlOptionalAssetsEditError};
pub use localization::{LocalizationCatalog, LocalizationError, UiTextKey};
pub use map16_controller::{Map16Controller, Map16ControllerEdit, Map16ControllerError};
pub use map16_document_controller::{
    Map16DocumentController, Map16DocumentControllerError, Map16DocumentEdit,
    Map16DocumentSaveSnapshot,
};
pub use map16_page_document_controller::{
    Map16PageDocumentController, Map16PageDocumentControllerError, Map16PageDocumentEdit,
    Map16PageDocumentSaveSnapshot,
};
pub use mwl_document_controller::{
    MwlDocumentController, MwlDocumentControllerError, MwlDocumentEdit, MwlDocumentSaveSnapshot,
};
pub use native_level_assets_controller::{
    NativeLevelAssetsController, NativeLevelAssetsControllerEdit, NativeLevelAssetsControllerError,
};
pub use native_level_assets_document_controller::{
    NativeLevelAssetsDocumentController, NativeLevelAssetsDocumentControllerError,
    NativeLevelAssetsDocumentSaveSnapshot,
};
pub use native_level_document_controller::{
    NativeLevelDocumentController, NativeLevelDocumentControllerError,
    NativeLevelDocumentSaveSnapshot,
};
pub use native_map16_sidecar_controller::{
    NativeMap16SidecarController, NativeMap16SidecarControllerError, NativeMap16SidecarDocument,
    NativeMap16SidecarDocumentKind, NativeMap16SidecarEdit, NativeMap16SidecarSaveSnapshot,
};
pub use osc_sidecar_controller::{
    OscSidecarController, OscSidecarControllerError, OscSidecarSaveSnapshot,
};
pub use overworld_appearance_document_controller::{
    OverworldAppearanceDocumentController, OverworldAppearanceDocumentControllerError,
    OverworldAppearanceDocumentEdit, OverworldAppearanceDocumentSaveSnapshot,
};
pub use overworld_controller::{
    OverworldController, OverworldControllerEdit, OverworldControllerError, OverworldLayerId,
};
pub use overworld_document_controller::{
    OverworldDocumentController, OverworldDocumentControllerError, OverworldDocumentSaveSnapshot,
};
pub use overworld_edit_batch::OverworldEditBatchError;
pub use overworld_metadata_controller::{
    OverworldMetadataController, OverworldMetadataControllerError, OverworldMetadataSaveSnapshot,
};
pub use overworld_path_controller::{
    OverworldPathController, OverworldPathControllerError, OverworldPathSaveSnapshot,
};
pub use palette_controller::{PaletteController, PaletteControllerEdit, PaletteControllerError};
pub use palette_document_controller::{
    PaletteDocumentController, PaletteDocumentControllerError, PaletteDocumentSaveSnapshot,
};
pub use palette_ownership_file::{PaletteOwnershipFile, PaletteOwnershipFileError};
pub use prepared_commit::PreparedRomCommit;
pub use profile_controller::{ProfileControllerError, RevisionProfileControllers};
pub use recent_documents::{RecentDocuments, RecentDocumentsError};
pub use revision_profile::{
    DirectTableAudit, PointerTableAudit, RevisionAllocationError, RevisionProfile,
    RevisionProfileAudit, RevisionProfileAuditError, RevisionProfileError,
    RevisionProfileReadError,
};
pub use selection::{EditorSelection, SelectionError};
pub use shortcut::{
    ShortcutBinding, ShortcutConfig, ShortcutError, ShortcutGesture, ShortcutKey, ShortcutModifiers,
};
pub use smw_map16_controller::{SmwMap16Controller, SmwMap16ControllerError};
pub use snapshot::{ControllerSnapshot, ProfiledControllerSnapshot};
pub use ssc_sidecar_controller::{
    SscSidecarController, SscSidecarControllerError, SscSidecarSaveSnapshot,
};
pub use state::{AppError, AppState};
pub use tool_config::{ToolConfig, ToolConfigError};
pub use toolbar::{ToolbarAction, ToolbarActivation, ToolbarConfig, ToolbarError, ToolbarItem};
pub use vanilla_entrance_controller::{VanillaEntranceController, VanillaEntranceControllerError};
pub use viewport_rendering::{
    EditorPreviewError, render_editor_preview, render_editor_viewport, render_level_viewport,
};
