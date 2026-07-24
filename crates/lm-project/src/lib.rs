//! Transactional project model and undo history.

mod credits_tilemap_io;
mod credits_tilemap_patch;
mod exanimation_io;
mod exanimation_slot_options_io;
mod expanded_settings_io;
mod graphics_io;
mod graphics_migration;
mod history;
mod installed_layout;
mod level_io;
mod level_layer2_io;
mod level_save;
mod lunar_magic_metadata_io;
mod map16_io;
mod map16_set_io;
mod mwl_exanimation;
mod mwl_optional_assets;
mod mwl_optional_assets_edit;
mod mwl_optional_assets_edit_script;
mod native_custom_overworld_sprite_io;
mod native_level_assets_file;
mod native_level_assets_layer2;
mod native_level_assets_load;
mod native_level_assets_save;
mod overworld_boss_sequence_patch;
mod overworld_endpoint_io;
mod overworld_event_io;
mod overworld_event_number_patch;
mod overworld_event_patch;
mod overworld_event_patch_save;
mod overworld_event_tilemap_patch;
mod overworld_file;
mod overworld_full_io;
mod overworld_io;
mod overworld_layer3_settings_io;
mod overworld_level_name_io;
mod overworld_level_name_patch_save;
mod overworld_message_io;
mod overworld_message_patch;
mod overworld_message_patch_save;
mod overworld_path_link_io;
mod overworld_path_patch;
mod overworld_path_patch_save;
mod overworld_player_start_io;
mod overworld_settings_io;
mod overworld_special_event_patch;
mod overworld_special_event_patch_save;
mod overworld_sprite_io;
mod overworld_warp_link_io;
mod overworld_warp_patch;
mod overworld_warp_patch_migrate;
mod overworld_warp_patch_save;
mod palette_io;
mod payload;
mod payload_load;
mod pointer_locator;
mod project;
mod rats_manifest_file;
mod rats_reclamation;
mod relocatable_patch;
mod rom_expansion;
mod secondary_exit_patch;
mod shared_palette_io;
mod title_recording_patch;
mod title_tilemap_patch;
mod transaction;

pub use credits_tilemap_io::{CreditsTilemapIoError, LegacyCreditsTilemapLayout};
pub use credits_tilemap_patch::{
    CreditsTilemapPatchError, CreditsTilemapPatchLocator, CreditsTilemapStorage,
    LoadedCreditsTilemap,
};
pub use exanimation_io::{
    ExAnimationIoError, ExAnimationRomLayout, ExAnimationSaveOptions, InstalledExAnimationRomLayout,
};
pub use exanimation_slot_options_io::{
    ExAnimationSlotOptionIoError, ExAnimationSlotOptionRomLayout, ExAnimationSlotOptionSaveOptions,
    LoadedExAnimationSlotOptions,
};
pub use expanded_settings_io::{ExpandedLevelSettingsIoError, ExpandedLevelSettingsLayout};
pub use graphics_io::{
    GraphicsCompression, GraphicsIoError, GraphicsRomLayout, GraphicsSaveOptions,
};
pub use graphics_migration::{
    GRAPHICS_COMPRESSION_MIGRATION_DESCRIPTION, GraphicsMigrationOptions,
};
pub use history::{CopierHeaderEdit, Edit, EditBatch, EditKind, History};
pub use installed_layout::{
    GatedLayout, InstallationMarker, InstalledAsset, InstalledLayout, InstalledLayoutError,
};
pub use level_io::{
    LevelLoadError, LevelPointerTable, LevelRomLayout, LoadedLevelSlot, SpritePointerTable,
};
pub use level_layer2_io::{
    LevelLayer2IoError, LevelLayer2RomLayout, LevelLayer2SaveOptions, LevelLayer2TilemapEncoding,
};
pub use level_save::{LevelSaveError, LevelSaveOptions, SavedLevelSlot};
pub use lunar_magic_metadata_io::{LunarMagicRomMetadataIoError, LunarMagicRomMetadataLayout};
pub use map16_io::{Map16IoError, Map16RomLayout, Map16SaveOptions, SavedMap16Page};
pub use map16_set_io::{Map16SetIoError, Map16SetSaveOptions, SavedMap16Set};
pub use mwl_exanimation::{MwlExAnimationSection, MwlExAnimationSectionError};
pub use mwl_optional_assets::{MwlOptionalLevelAssets, MwlOptionalLevelAssetsError};
pub use mwl_optional_assets_edit::{
    MwlOptionalAssetsEdit, MwlOptionalAssetsEditError, apply_mwl_optional_assets_edit,
};
pub use mwl_optional_assets_edit_script::{
    EditScriptError as MwlOptionalAssetsEditScriptError, MAGIC as MWL_OPTIONAL_ASSETS_EDIT_MAGIC,
    MAX_SCRIPT_LEN as MAX_MWL_OPTIONAL_ASSETS_EDIT_SCRIPT_LEN,
    parse as parse_mwl_optional_assets_edit_script,
};
pub use native_custom_overworld_sprite_io::{
    LoadedNativeCustomOverworldSprites, NativeCustomOverworldSpriteIoError,
    NativeCustomOverworldSpriteRomLayout, NativeCustomOverworldSpriteSaveOptions,
};
pub use native_level_assets_file::{NativeLevelAssetsFile, NativeLevelAssetsFileError};
pub use native_level_assets_layer2::{
    LoadedNativeLevelAssetsLayer2, NativeLevelAssetsLayer2, NativeLevelAssetsLayer2Layout,
    NativeLevelAssetsLayer2LoadError, NativeLevelAssetsLayer2SaveError,
    NativeLevelAssetsLayer2SaveOptions, SavedNativeLevelAssetsLayer2,
};
pub use native_level_assets_load::{LoadedNativeLevelAssets, NativeLevelAssetsLoadError};
pub use native_level_assets_save::{
    NativeLevelAssets, NativeLevelAssetsLayout, NativeLevelAssetsSaveError,
    NativeLevelAssetsSaveOptions, SavedNativeLevelAssets,
};
pub use overworld_boss_sequence_patch::{
    BossSequencePatchError, BossSequencePatchLocator, BossSequenceStorage,
    LoadedBossSequenceMessages,
};
pub use overworld_endpoint_io::{EndpointIoError, EndpointRomLayout, EndpointSaveOptions};
pub use overworld_event_io::{
    EventRevealIoError, EventRevealRomLayout, EventRevealSaveOptions, SavedEventRevealTable,
};
pub use overworld_event_number_patch::{
    LoadedOverworldEventNumberMap, OverworldEventNumberMapError, OverworldEventNumberMapLocator,
    OverworldEventNumberMapStorage,
};
pub use overworld_event_patch::{
    LoadedOverworldEventReveals, OverworldEventRevealLocator, OverworldEventRevealPatchError,
    OverworldEventRevealStorage,
};
pub use overworld_event_patch_save::OverworldEventRevealSaveError;
pub use overworld_event_tilemap_patch::{
    EventTilemapCompression, EventTilemapPatchError, EventTilemapPatchLocator,
    LoadedEventTilemapBuffers,
};
pub use overworld_file::{
    CompleteOverworldFile, CompleteOverworldFileError, CompleteOverworldShape,
};
pub use overworld_full_io::{
    CompleteOverworldData, CompleteOverworldIoError, CompleteOverworldRomLayout,
    CompleteOverworldSaveOptions, SavedCompleteOverworld,
};
pub use overworld_io::{
    OverworldIoError, OverworldLayers, OverworldLayersRomLayout, OverworldSaveOptions,
    SavedOverworldLayers,
};
pub use overworld_layer3_settings_io::{
    OverworldLayer3SettingsIoError, OverworldLayer3SettingsRomLayout,
};
pub use overworld_level_name_io::{
    LoadedOverworldLevelNames, OverworldLevelNameIoError, OverworldLevelNameLocator,
    OverworldLevelNameStorage,
};
pub use overworld_level_name_patch_save::OverworldLevelNamePatchSaveError;
pub use overworld_message_io::{MessageIoError, MessageRomLayout, MessageSaveOptions};
pub use overworld_message_patch::{
    ExpandedOverworldMessageStorage, LoadedExpandedOverworldMessages, OverworldMessagePatchError,
    OverworldMessagePatchLocator,
};
pub use overworld_message_patch_save::OverworldMessagePatchSaveError;
pub use overworld_path_link_io::{OverworldPathLinkIoError, OverworldPathLinkRomLayout};
pub use overworld_path_patch::{
    LoadedOverworldPathLinks, OverworldPathLinkStorage, OverworldPathPatchError,
    OverworldPathPatchLocator,
};
pub use overworld_path_patch_save::OverworldPathPatchSaveError;
pub use overworld_player_start_io::{OverworldPlayerStartIoError, OverworldPlayerStartRomLayout};
pub use overworld_settings_io::ExpandedOverworldSettingsIoError;
pub use overworld_special_event_patch::{
    LoadedSpecialEventReveals, SpecialEventRevealPatchError, SpecialEventRevealPatchLocator,
    SpecialEventRevealStorage,
};
pub use overworld_special_event_patch_save::SpecialEventRevealSaveError;
pub use overworld_sprite_io::{SpriteIoError, SpriteRomLayout, SpriteSaveOptions};
pub use overworld_warp_link_io::{OverworldWarpLinkIoError, OverworldWarpLinkRomLayout};
pub use overworld_warp_patch::{
    LoadedOverworldWarpLinks, OverworldWarpLinkStorage, OverworldWarpPatchError,
    OverworldWarpPatchLocator,
};
pub use overworld_warp_patch_migrate::{
    OverworldWarpPatchMigrationError, OverworldWarpPatchMigrationOptions,
};
pub use overworld_warp_patch_save::OverworldWarpPatchSaveError;
pub use palette_io::{PaletteIoError, PaletteRomLayout, PaletteSaveOptions};
pub use payload::{
    PayloadPointer, PayloadReclamation, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult,
};
pub use payload_load::{LoadedPayload, PayloadLoadError, PayloadReadPolicy};
pub use pointer_locator::{ChainedSnesPointerLocator, PointerLocatorError};
pub use project::{Project, RomMutation, RomWrite};
pub use rats_manifest_file::{RatsManifestFileError, RatsOwnershipManifestFile};
pub use rats_reclamation::{RatsOwnershipManifest, RatsReclamationError, RatsReclamationPlan};
pub use relocatable_patch::{
    PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, RelocatablePatchError,
    RelocatablePatchGroupError, RelocatablePatchPlan, RelocatablePatchResult,
};
pub use secondary_exit_patch::{
    LoadedSecondaryExitTable, SecondaryExitPatchError, SecondaryExitPatchLocator,
    SecondaryExitStorage,
};
pub use shared_palette_io::{SharedPaletteIoError, SharedPaletteRomLayout};
pub use title_recording_patch::{
    LoadedTitleRecording, TitleRecordingPatchError, TitleRecordingPatchLocator,
    TitleRecordingStorage,
};
pub use title_tilemap_patch::{
    LoadedTitleTilemap, TitleTilemapPatchError, TitleTilemapPatchLocator, TitleTilemapStorage,
};
pub use transaction::{RomTransaction, TransactionError};
