//! Lossless, platform-neutral level data model.

mod binary;
mod complete_file;
mod custom_object_library;
mod custom_sprite_library;
mod dsc_display;
mod dsc_materialize;
mod dsc_sidecar;
mod dsc_sidecar_resolved;
mod editing;
mod entrance;
mod expanded_settings;
mod expanded_settings_layer3;
mod header;
mod layer3;
mod layer3_editing;
mod layer3_tilemap_workspace;
mod lm16_map16_file;
mod map16;
mod map16_editing;
mod map16_file;
mod map16_set;
mod map16_set_file;
mod model;
mod mwl;
mod native_file;
mod native_layer2;
mod native_map16_sidecar;
mod native_sprite;
mod object;
mod object_editing;
mod object_fields;
mod object_placement;
mod object_relocation;
mod osc_sidecar;
mod osc_sidecar_resolved;
mod overworld_settings;
mod property_editing;
mod sprite;
mod sprite_editing;
mod sprite_placement;
mod ssc_sidecar;
mod ssc_sidecar_resolved;

pub use binary::{BinaryError, ByteCursor};
pub use complete_file::{CompleteLevelFile, CompleteLevelFileError, LevelCollection};
pub use custom_object_library::{
    CustomObjectEntry, CustomObjectLibrary, CustomObjectLibraryError, DescriptionFormat,
    LineEnding, MAX_CUSTOM_OBJECT_SIDECAR_LEN,
};
pub use custom_sprite_library::{
    CustomSpriteEntry, CustomSpriteLibrary, CustomSpriteLibraryError, MAX_CUSTOM_SPRITE_SIDECAR_LEN,
};
pub use dsc_display::{DscDisplayContext, DscDisplayResolution};
pub use dsc_materialize::{DscMaterialization, DscMaterializationContext, DscMaterializationError};
pub use dsc_sidecar::{
    DscDescription, DscDirective, DscEntry, DscSidecar, DscSidecarError, MAX_DSC_SOURCE_LEN,
};
pub use dsc_sidecar_resolved::{DscDescriptionStyle, DscResolvedEntry, DscResolvedTable};
pub use editing::LevelEditError;
pub use entrance::{
    Entrance, EntranceKind, MwlSecondaryExit, MwlSecondaryExitDecodeError, ScreenExit,
    SecondaryExit, SecondaryExitEncodingError, SecondaryExitTable, SecondaryExitTableFileError,
    SeparateMidwayEntrance, SeparateMidwayEntranceTable, SeparateMidwayEntranceTableError,
};
pub use expanded_settings::{ExpandedLevelSettingsError, ExpandedLevelSettingsRecord};
pub use expanded_settings_layer3::{
    Layer3TilemapGraphicsDescriptor, Layer3TilemapGraphicsDescriptorError,
};
pub use header::{ExpandedLevelHeader, HeaderValueError, LegacyLevelHeader, LevelHeader};
pub use layer3::{Layer3Data, Layer3Error, Layer3File, Layer3Settings};
pub use layer3_editing::{Layer3Edit, Layer3EditError};
pub use layer3_tilemap_workspace::{
    LAYER3_TILEMAP_WORKSPACE_LEN, Layer3TilemapWorkspace, Layer3TilemapWorkspaceError,
};
pub use lm16_map16_file::{
    Lm16Map16File, Lm16Map16FileError, Lm16Map16Section, Lm16Map16SectionKind,
};
pub use map16::{Map16Page, Map16PageEncodingError, Map16Tile, Subtile};
pub use map16_editing::{Map16Address, Map16EditError, Map16Quadrant};
pub use map16_file::{Map16PageFile, Map16PageFileError};
pub use map16_set::{ActsLikeResolution, Map16Set, Map16SetError};
pub use map16_set_file::{Map16SetFile, Map16SetFileError};
pub use model::{LayerData, Level};
pub use mwl::{
    MwlError, MwlFile, MwlLevelHeaderSection, MwlMainEntranceSettings, MwlMidwayEntranceSettings,
    MwlPaletteSection, MwlPaletteSectionError, MwlPayloadSection, MwlSection, MwlSectionKind,
};
pub use native_file::{NativeLevelFile, NativeLevelFileError, StreamKind};
pub use native_layer2::{
    LEGACY_LAYER2_TILEMAP_LEN, Layer2Storage, NATIVE_LAYER2_TILEMAP_HEIGHT,
    NATIVE_LAYER2_TILEMAP_LEN, NATIVE_LAYER2_TILEMAP_WIDTH, NativeLayer2Data, NativeLayer2Error,
    compact_legacy_layer2_tilemap, expand_legacy_layer2_tilemap, interleave_layer2_tilemap_planes,
    level_mode_layer2_storage, native_layer2_flood_region, native_layer2_tilemap_index,
    split_layer2_tilemap_planes,
};
pub use native_map16_sidecar::{M16Sidecar, NativeMap16SidecarError, S16Sidecar};
pub use native_sprite::{
    NativeSpriteEncodingError, NativeSpriteFieldError, NativeSpriteRecordFields,
    NativeSpriteStream, SpriteLengthTable, SpriteLengthTableError, SpriteToken,
};
pub use object::{
    LevelObjectData, ObjectRecord, ObjectStream, ObjectStreamError, encoded_record_length,
};
pub use object_editing::{ObjectEdit, ObjectEditError};
pub use object_fields::{
    ObjectCoordinateNibbles, ObjectFieldError, ObjectScreenExit, ObjectScreenJump,
    ScreenExitObjectEncoding, ScreenJumpEncoding,
};
pub use object_placement::NativeObjectPlacement;
pub use object_relocation::ObjectRelocationError;
pub use osc_sidecar::{
    MAX_OSC_ATTRIBUTES, MAX_OSC_DISPLAY_TILES, MAX_OSC_SOURCE_LEN, MAX_OSC_VALUE_RECORDS,
    OscDirective, OscDisplayTile, OscEntry, OscObjectSelector, OscSidecar, OscSidecarError,
};
pub use osc_sidecar_resolved::{OscResolvedObject, OscResolvedTable};
pub use overworld_settings::{ExpandedOverworldSettings, ExpandedOverworldSettingsError};
pub use property_editing::{
    LayerDimensions, LegacyHeaderEdit, LevelLayer, LevelPropertyEdit, LevelPropertyEditError,
    TileCoordinate,
};
pub use sprite::{SpriteRecord, SpriteStream, SpriteStreamError};
pub use sprite_editing::{SpriteEdit, SpriteEditError, SpriteEditLimits};
pub use sprite_placement::NativeSpritePlacement;
pub use ssc_sidecar::{
    MAX_SSC_DISPLAY_TILES, MAX_SSC_PALETTE_RECORDS, MAX_SSC_SOURCE_LEN, SscDirective,
    SscDisplayTile, SscEntry, SscRemapRange, SscSidecar, SscSidecarError, SscSpriteSelector,
};
pub use ssc_sidecar_resolved::{SSC_REMAP_ENTRY_COUNT, SscResolvedSprite, SscResolvedTable};
mod appearance_file;
mod auxiliary_edit_script;
mod auxiliary_editing;
pub use appearance_file::{
    AppearanceSource, EntityAppearanceFile, EntityAppearanceFileError, EntityAppearanceRecord,
};
pub use auxiliary_edit_script::{
    AUXILIARY_EDIT_SCRIPT_MAGIC, AuxiliaryEditScriptError, MAX_AUXILIARY_EDIT_COMMANDS,
    MAX_AUXILIARY_EDIT_LINE_BYTES, MAX_AUXILIARY_EDIT_SCRIPT_BYTES, parse_auxiliary_edit_script,
};
pub use auxiliary_editing::{
    AuxiliaryCollection, LevelAuxiliaryEdit, LevelAuxiliaryEditError, Map16OverrideEdit,
    SequenceEdit,
};
