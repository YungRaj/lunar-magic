//! Lossless overworld models and recovered fixed-size records.

mod appearance;
mod boss_sequence;
mod credits_tilemap;
mod editing;
mod endpoint;
mod event;
mod event_tilemap;
mod layer_tilemap;
mod message;
mod metadata;
mod metadata_editing;
mod metadata_file;
mod model;
mod native_boss_sequence_file;
mod native_credits_tilemap_file;
mod native_custom_sprite;
mod native_event_file;
mod native_event_number_file;
mod native_event_tilemap_file;
mod native_layer3_settings;
mod native_layer_tilemap_file;
mod native_level_name;
mod native_message_file;
mod native_path;
mod native_path_file;
mod native_player_start;
mod native_special_event_file;
mod native_sprite_sidecar;
mod native_warp;
mod native_warp_file;
mod path;
mod path_editing;
mod path_file;
mod special_event;
mod sprite;
mod table_encoding;

pub use appearance::{
    SpriteAppearanceDefinition, SpriteAppearanceFile, SpriteAppearanceFileError,
    SpriteAppearancePart,
};
pub use boss_sequence::{BossSequenceMessageTable, BossSequenceTableError};
pub use credits_tilemap::{CreditsTilemap, CreditsTilemapError, EncodedCreditsRows};
pub use editing::{
    OverworldEditError, OverworldRecord, insert_record, move_record_before, remove_record,
};
pub use endpoint::OverworldEndpoint;
pub use event::{
    EventId, EventNumberMap, EventReveal, EventRevealMoveError, EventRevealTable, EventTableError,
    EventTileChange, decode_main_overworld_event_tile_index,
    encode_main_overworld_event_tile_index,
};
pub use event_tilemap::{EventTilemapBufferError, EventTilemapBuffers};
pub use layer_tilemap::{ExpandedLayerTilemap, ExpandedLayerTilemapError};
pub use message::{
    BossSequenceMessage, OverworldMessage, VanillaOverworldMessageError,
    decode_vanilla_overworld_messages,
};
pub use metadata::{
    MetadataError, OverworldLevelName, OverworldMetadata, PlayerStart, SubmapSettings,
};
pub use metadata_editing::{MetadataEdit, MetadataEditError};
pub use metadata_file::MetadataFileError;
pub use model::{Overworld, OverworldLayer, OverworldLayerEncodingError, Submap};
pub use native_boss_sequence_file::BossSequenceFileError;
pub use native_credits_tilemap_file::CreditsTilemapFileError;
pub use native_custom_sprite::{
    CUSTOM_OVERWORLD_MAP_COUNT, CUSTOM_OVERWORLD_SPRITE_ID_COUNT, CUSTOM_OVERWORLD_SPRITES_PER_MAP,
    NativeCustomOverworldSprite, NativeCustomOverworldSpriteError,
    NativeCustomOverworldSpriteTable,
};
pub use native_event_file::OverworldEventFileError;
pub use native_event_number_file::OverworldEventNumberFileError;
pub use native_event_tilemap_file::EventTilemapFileError;
pub use native_layer_tilemap_file::ExpandedLayerTilemapFileError;
pub use native_layer3_settings::{
    OVERWORLD_LAYER3_GFX_SLOTS, OVERWORLD_LAYER3_LAYOUT_WORDS, OVERWORLD_LAYER3_MAP_COUNT,
    OverworldLayer3SettingsError, OverworldLayer3SettingsRecord, OverworldLayer3SettingsTable,
};
pub use native_level_name::{NativeOverworldLevelNameError, NativeOverworldLevelNameTable};
pub use native_message_file::{
    OverworldMessageFileError, decode_native_overworld_message_file,
    encode_native_overworld_message_file,
};
pub use native_path::{
    LUNAR_MAGIC_EXIT_PATH_TILE_TYPES, LUNAR_MAGIC_EXIT_TILE_TYPES, OverworldPathDirection,
    OverworldPathLink, OverworldPathLinkPlanes, OverworldPathLinkTable,
    OverworldPathLinkTableError, OverworldPathTarget, is_lunar_magic_exit_path_tile_type,
    is_lunar_magic_exit_tile_type,
};
pub use native_path_file::OverworldPathLinkFileError;
pub use native_player_start::{NativeOverworldPlayerStartError, NativeOverworldPlayerStarts};
pub use native_special_event_file::SpecialEventRevealFileError;
pub use native_sprite_sidecar::{
    NativeOverworldSpriteAppearance, NativeOverworldSpriteDisplay, NativeOverworldSpriteMap16Part,
    NativeOverworldSpriteRange, NativeOverworldSpriteSidecar, NativeOverworldSpriteSidecarError,
    NativeOverworldSpriteTooltip, SSCOV_MAX_ABSOLUTE_OFFSET, SSCOV_MAX_BYTES, SSCOV_MAX_PARTS,
    SSCOV_MAX_SPRITE_MAP16_TILE,
};
pub use native_warp::{
    OverworldWarpEndpoint, OverworldWarpLink, OverworldWarpLinkPlanes, OverworldWarpLinkTable,
    OverworldWarpLinkTableError, OverworldWarpReturnChoice,
};
pub use native_warp_file::OverworldWarpLinkFileError;
pub use path::{OverworldPathGraph, PathDirection, PathEdge, PathGraphError, PathNode};
pub use path_editing::{PathGraphEdit, PathGraphEditError};
pub use path_file::PathFileError;
pub use special_event::{
    SpecialEventRevealError, SpecialEventRevealPlanes, SpecialEventRevealTable,
};
pub use sprite::{OverworldSprite, OverworldSpriteError};
pub use table_encoding::FixedTableEncodingError;
