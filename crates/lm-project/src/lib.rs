//! Transactional project model and undo history.

mod credits_tilemap_io;
mod credits_tilemap_patch;
mod exanimation_feature_io;
mod exanimation_io;
mod exanimation_slot_options_io;
mod exlorom_conversion;
mod expanded_level_mode_io;
mod expanded_settings_io;
mod graphics_io;
mod graphics_migration;
mod history;
mod installed_layout;
mod legacy_exanimation_io;
mod legacy_mwl;
mod level_access_restriction;
mod level_io;
mod level_layer2_io;
mod level_save;
mod lfix3_level_fields_io;
mod lunar_magic_metadata_io;
mod map16_bitmap_import_io;
mod map16_io;
mod map16_set_io;
mod mwl_exanimation;
mod mwl_native_level;
mod mwl_optional_assets;
mod mwl_optional_assets_edit;
mod mwl_optional_assets_edit_script;
mod native_custom_overworld_sprite_io;
mod native_level_assets_file;
mod native_level_assets_layer2;
mod native_level_assets_load;
mod native_level_assets_save;
mod overworld_animation_options_io;
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
mod restore_archive;
mod rom_expansion;
mod sa1_expansion;
mod secondary_exit_patch;
mod separate_midway_patch;
mod shared_palette_io;
mod super_graphics_io;
mod title_recording_patch;
mod title_recording_recorder;
mod title_tilemap_patch;
mod transaction;
mod vanilla_entrance_io;

pub use credits_tilemap_io::{CreditsTilemapIoError, LegacyCreditsTilemapLayout};
pub use credits_tilemap_patch::{
    CreditsTilemapPatchError, CreditsTilemapPatchLocator, CreditsTilemapStorage,
    LoadedCreditsTilemap,
};
pub use exanimation_feature_io::{
    EXANIMATION_FEATURE_LEVEL_COUNT, ExAnimationFeatureIoError, ExAnimationFeatureRomLayout,
    ExAnimationFeatureStorage, ExAnimationFeatureWritePlan, InstalledExAnimationFeatureRomLayout,
    LEGACY_SPECIAL_LEVEL, LEGACY_SPECIAL_LEVEL_FEATURE_BYTE, LoadedExAnimationFeatures,
    ResolvedExAnimationFeatureRomLayout,
};
pub use exanimation_io::{
    ExAnimationIoError, ExAnimationRomLayout, ExAnimationSaveOptions,
    InstalledExAnimationRomLayout, LoadedInstalledGlobalExAnimation,
};
pub use exanimation_slot_options_io::{
    ExAnimationSlotOptionIoError, ExAnimationSlotOptionRomLayout, ExAnimationSlotOptionSaveOptions,
    LoadedExAnimationSlotOptions,
};
pub use exlorom_conversion::{EXLOROM_CONVERSION_TARGET_LEN, ExLoRomConversionError};
pub use expanded_level_mode_io::{ExpandedLevelModeIoError, ExpandedLevelModeLocator};
pub use expanded_settings_io::{ExpandedLevelSettingsIoError, ExpandedLevelSettingsLayout};
pub use graphics_io::{
    GraphicsCompression, GraphicsIoError, GraphicsPointerPlanes, GraphicsRomLayout,
    GraphicsSaveOptions,
};
pub use graphics_migration::{
    GRAPHICS_COMPRESSION_MIGRATION_DESCRIPTION, GraphicsMigrationOptions,
};
pub use history::{CopierHeaderEdit, Edit, EditBatch, EditKind, History};
pub use installed_layout::{
    GatedLayout, InstallationMarker, InstalledAsset, InstalledLayout, InstalledLayoutError,
};
pub use legacy_exanimation_io::{
    LEGACY_EXANIMATION_LEVEL_COUNT, LegacyExAnimationIoError, LegacyExAnimationMigrationLayout,
    LegacyExAnimationMigrationResult, LegacyExAnimationRomLayout, LoadedLegacyExAnimationSlot,
};
pub use legacy_mwl::{LegacyMwlBundle, LegacyMwlBundleError};
pub use level_access_restriction::{
    ExLoRomRestrictionBulkSaveLayout, LevelAccessRestrictionError, LevelAccessRestrictionKeys,
    LevelAccessRestrictionLayout, LevelAccessRestrictionPrerequisitePatch,
};
pub use level_io::{
    LevelLoadError, LevelPointerTable, LevelRomLayout, LoadedLevelSlot, SpritePointerTable,
};
pub use level_layer2_io::{
    LevelLayer2DescriptorTable, LevelLayer2IoError, LevelLayer2PointerRedirect,
    LevelLayer2RomLayout, LevelLayer2SaveOptions, LevelLayer2TilemapEncoding, LoadedLevelLayer2,
};
pub use level_save::{LevelSaveError, LevelSaveOptions, SavedLevelSlot};
pub use lfix3_level_fields_io::{
    Lfix3LevelFields, Lfix3LevelFieldsIoError, Lfix3LevelFieldsRomLayout,
};
pub use lunar_magic_metadata_io::{LunarMagicRomMetadataIoError, LunarMagicRomMetadataLayout};
pub use map16_bitmap_import_io::{
    Map16BitmapGraphicsSave, Map16BitmapPageSave, Map16BitmapPaletteSave, Map16BitmapRomSave,
    Map16BitmapRomSaveError, SavedMap16BitmapImport,
};
pub use map16_io::{Map16IoError, Map16RomLayout, Map16SaveOptions, SavedMap16Page};
pub use map16_set_io::{Map16SetIoError, Map16SetSaveOptions, SavedMap16Set};
pub use mwl_exanimation::{MwlExAnimationSection, MwlExAnimationSectionError};
pub use mwl_native_level::{MwlNativeLevel, MwlNativeLevelError};
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
pub use overworld_animation_options_io::{
    LoadedOverworldAnimationOptions, OVERWORLD_ANIMATION_MAP_COUNT,
    OverworldAnimationOptionsIoError, OverworldAnimationOptionsRomLayout,
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
    RelocatablePatchGroupError, RelocatablePatchPlan, RelocatablePatchReplacementError,
    RelocatablePatchResult,
};
pub use restore_archive::{
    LUNAR_RESTORE_ASSOCIATED_EXTENSIONS, LUNAR_RESTORE_ASSOCIATED_FILE_COUNT, LunarRestoreArchive,
    LunarRestoreArchiveCreateRequest, LunarRestoreArchiveError, LunarRestoreArchiveHeader,
    LunarRestoreAssociatedFileEntry, LunarRestoreAutomaticDecision,
    LunarRestoreAutomaticFullReason, LunarRestoreAutomaticPolicy, LunarRestoreCommand,
    LunarRestorePointRecord, LunarRestoreReversionRequest, LunarRestoredAssociatedFile,
    PackedRestoreDate, PackedRestoreTime,
};
pub use sa1_expansion::{SA1_6_MIB_LEN, SA1_8_MIB_LEN, Sa1ExpansionError};
pub use secondary_exit_patch::{
    LoadedSecondaryExitTable, SecondaryExitPatchError, SecondaryExitPatchLocator,
    SecondaryExitStorage,
};
pub use separate_midway_patch::{
    LoadedSeparateMidwayTable, SeparateMidwayPatchError, SeparateMidwayPatchLocator,
};
pub use shared_palette_io::{SharedPaletteIoError, SharedPaletteRomLayout};
pub use super_graphics_io::{
    LoadedSuperGraphicsBypass, LoadedSuperGraphicsSlot, SuperGraphicsIoError,
};
pub use title_recording_patch::{
    LoadedTitleRecording, TitleRecordingExpansionWrite, TitleRecordingPatchError,
    TitleRecordingPatchLocator, TitleRecordingStorage,
};
pub use title_recording_recorder::{
    TitleRecordingRecorderError, TitleRecordingRecorderLocator, TitleRecordingRecorderState,
};
pub use title_tilemap_patch::{
    LoadedTitleTilemap, TitleTilemapPatchError, TitleTilemapPatchLocator, TitleTilemapStorage,
};
pub use transaction::{RomTransaction, TransactionError};
pub use vanilla_entrance_io::{
    VanillaEntranceIoError, VanillaEntranceRomLayout, VanillaMainEntrance,
};
