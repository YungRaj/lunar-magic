//! Strict, identity-bound external revision metadata shared by editor frontends.

mod allocation;
mod audit;
mod credits_tilemap;
mod expanded_settings_allocation;
mod expanded_settings_base;
mod expanded_settings_hooks;
mod expanded_settings_install;
mod expanded_settings_runtime;
mod expanded_settings_runtime_21c;
mod expanded_settings_runtime_bundle;
mod layer3_compatibility;
mod layer3_dispatch_code;
mod layer3_extended_runtime;
mod layer3_install;
mod layer3_main_patch;
mod layer3_main_patch_install;
mod layer3_main_runtime;
mod layer3_runtime;
mod layer3_scroll;
mod layer3_scroll_code;
mod lfix3_install;
mod lfix3_runtime;
mod lunar_magic_metadata;
mod native_assets;
mod native_map16_remap;
mod native_map16_transfer;
mod overworld_boss_sequence;
mod overworld_event;
mod overworld_event_number;
mod overworld_event_tilemap;
mod overworld_level_name;
mod overworld_message_patch;
mod overworld_path;
mod overworld_path_patch;
mod overworld_player_start;
mod overworld_settings;
mod overworld_special_event;
mod overworld_warp;
mod overworld_warp_patch;
mod reader;
mod revision_patch;
mod secondary_exit;
mod secondary_exit_install;
mod secondary_exit_runtime;
mod shared_palette;
mod shared_palette_install;
mod text;
mod text_encode;
mod text_schema;
mod title_recording;
mod title_tilemap;
mod vanilla_level_map16;
mod vanilla_smw;

use lm_level::SpriteLengthTable;
use lm_project::{
    CompleteOverworldRomLayout, CompleteOverworldShape, ExAnimationRomLayout,
    ExpandedLevelSettingsLayout, GraphicsRomLayout, InstalledExAnimationRomLayout, InstalledLayout,
    LevelLayer2RomLayout, LevelPointerTable, LevelRomLayout, Map16RomLayout, PaletteRomLayout,
};
use lm_rom::{Mapper, Region, RomIdentity, SupportedGame, pc_to_snes};
use std::fmt;
use std::ops::Range;

pub use allocation::RevisionAllocationError;
pub use audit::{
    DirectTableAudit, PointerTableAudit, RevisionProfileAudit, RevisionProfileAuditError,
};
pub use credits_tilemap::{
    SMW_US_V1_CREDITS_BLANK_WORD, SMW_US_V1_CREDITS_EXPANDED_OFFSETS_OFFSET,
    SMW_US_V1_CREDITS_LEGACY_ROWS, SMW_US_V1_CREDITS_OFFSETS_OFFSET,
    SMW_US_V1_CREDITS_RECORDS_OFFSET, SMW_US_V1_CREDITS_RUNTIME_OFFSET,
    SMW_US_V1_CREDITS_SEARCH_START, smw_us_v1_credits_allocation_policy,
    smw_us_v1_credits_tilemap_locator, smw_us_v1_legacy_credits_tilemap_layout,
};
pub use expanded_settings_allocation::{
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN, SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT, SMW_US_V1_EXPANDED_SETTINGS_SPECIAL_RECORD_OFFSET,
    SMW_US_V1_EXPANDED_SETTINGS_STANDARD_LEVEL_COUNT, SmwUsV1ExpandedSettingsAllocation,
    SmwUsV1ExpandedSettingsAllocationError, smw_us_v1_default_expanded_settings_record,
    smw_us_v1_default_special_expanded_settings_record,
    smw_us_v1_normalize_expanded_settings_record,
};
pub use expanded_settings_base::{
    ExpandedSettingsBaseError, SMW_US_V1_EXPANDED_SETTINGS_BASE_HELPER_OFFSET,
    SMW_US_V1_EXPANDED_SETTINGS_BASE_HOOK_OFFSET, SMW_US_V1_EXPANDED_SETTINGS_BASE_POINTER_OFFSETS,
    smw_us_v1_expanded_settings_base_helper, smw_us_v1_expanded_settings_base_writes,
};
pub use expanded_settings_hooks::{
    ExpandedSettingsHook, ExpandedSettingsHookError, ExpandedSettingsOperandRelocation,
    SMW_US_V1_EXPANDED_SETTINGS_DIRECT_HOOKS, SMW_US_V1_EXPANDED_SETTINGS_OPERAND_RELOCATION,
    smw_us_v1_expanded_settings_direct_hook_writes,
    smw_us_v1_expanded_settings_operand_relocation_write,
};
pub use expanded_settings_install::{
    ExpandedSettingsInstallPlanError, SMW_US_V1_CHECKSUM_FIELD,
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_END,
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START,
    smw_us_v1_expanded_settings_installation_plan,
    smw_us_v1_expanded_settings_installation_plan_with_overworld_settings,
};
pub use expanded_settings_runtime::{
    ExpandedSettingsEntryContinuation, ExpandedSettingsRelocation,
    ExpandedSettingsRelocationTarget, ExpandedSettingsRuntimeBlock,
    ExpandedSettingsRuntimeBuildError, RuntimeBlockVerificationError, RuntimeMutableSpan,
    SMW_US_V1_EXPANDED_HEADER_FIXED_RUNTIME_RELOCATIONS,
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_RELOCATIONS, SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_BLOCKS,
    SMW_US_V1_GENERATED_EXPANDED_SETTINGS_RUNTIME_BLOCKS,
    smw_us_v1_expanded_settings_allocation_load_block, smw_us_v1_expanded_settings_dma_block,
    smw_us_v1_expanded_settings_field_runtime_block,
    smw_us_v1_expanded_settings_index_restore_block,
    smw_us_v1_expanded_settings_indexed_scratch_block,
    smw_us_v1_expanded_settings_pointer_dispatch_block,
    smw_us_v1_expanded_settings_record_select_block, smw_us_v1_expanded_settings_reset_block,
    smw_us_v1_expanded_settings_selector_dispatch_block,
    smw_us_v1_expanded_settings_special_record_block,
    smw_us_v1_expanded_settings_state_compare_block, verify_expanded_settings_runtime_block,
};
pub use expanded_settings_runtime_21c::{
    ExpandedSettingsTransferRuntimeError, smw_us_v1_expanded_settings_transfer_runtime_block,
};
pub use expanded_settings_runtime_bundle::{
    ExpandedSettingsAllocationFixup, ExpandedSettingsAllocationFixupEncoding,
    ExpandedSettingsRuntimeBundleError, ExpandedSettingsRuntimeComponent,
    ExpandedSettingsRuntimeLayout, SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_ALLOCATION_FIXUPS,
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_DESTINATIONS, resolve_expanded_settings_runtime_allocation,
    smw_us_v1_expanded_settings_fixed_writes, smw_us_v1_expanded_settings_runtime_bundle,
    smw_us_v1_expanded_settings_runtime_writes,
};
pub use layer3_compatibility::{
    Layer3CompatibilityBuildError, SMW_US_V1_LAYER3_AUXILIARY_PAYLOAD_LEN,
    SMW_US_V1_LAYER3_COMPATIBILITY_PAYLOAD_LEN, SMW_US_V1_LAYER3_COMPATIBILITY_SEARCH_END,
    SMW_US_V1_LAYER3_COMPATIBILITY_SEARCH_START, smw_us_v1_layer3_auxiliary_payload,
    smw_us_v1_layer3_compatibility_installation_plan, smw_us_v1_layer3_compatibility_payload,
};
pub use layer3_dispatch_code::{
    Layer3ScrollDispatchProgram, smw_us_v1_layer3_scroll_dispatch_program,
};
pub use layer3_extended_runtime::{
    SMW_US_V1_LAYER3_EXTENDED_RUNTIME_LEN, SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_END,
    SMW_US_V1_LAYER3_EXTENDED_RUNTIME_SEARCH_START,
    smw_us_v1_layer3_extended_runtime_installation_plan, smw_us_v1_layer3_extended_runtime_payload,
    smw_us_v1_layer3_extended_runtime_writes,
};
pub use layer3_install::{
    CompleteLayer3BuildError, smw_us_v1_complete_layer3_feature_plans,
    smw_us_v1_complete_layer3_installation_plan,
};
pub use layer3_main_patch::{
    Layer3MainEntry, SMW_US_V1_LAYER3_LEVEL_DISPATCH_ENTRY, SMW_US_V1_LAYER3_MAIN_ENTRIES,
    SMW_US_V1_LAYER3_MAIN_PAYLOAD_LEN, SMW_US_V1_LAYER3_MODE_DISPATCH_ENTRY,
    SMW_US_V1_LAYER3_MODE_VALUE_ENTRY, SMW_US_V1_LAYER3_STATUS_ENTRY,
};
pub use layer3_main_patch_install::{
    SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_END, SMW_US_V1_LAYER3_MAIN_PATCH_SEARCH_START,
    smw_us_v1_layer3_main_patch_installation_plan, smw_us_v1_layer3_main_patch_payload,
    smw_us_v1_layer3_main_patch_writes,
};
pub use layer3_main_runtime::{
    SMW_US_V1_LAYER3_MAIN_RUNTIME_CODE_OFFSET, SMW_US_V1_LAYER3_MAIN_RUNTIME_LEN,
    SMW_US_V1_LAYER3_MAIN_RUNTIME_LEVEL_OFFSETS, SMW_US_V1_LAYER3_MAIN_RUNTIME_SEARCH_END,
    SMW_US_V1_LAYER3_MAIN_RUNTIME_SEARCH_START, SMW_US_V1_LAYER3_MAIN_RUNTIME_SHARED_HELPER_OFFSET,
    SMW_US_V1_LAYER3_MAIN_RUNTIME_TABLE_OFFSET, SMW_US_V1_LAYER3_MAIN_RUNTIME_WORKSPACE_LEN,
    smw_us_v1_layer3_main_runtime_allocation_hooks,
    smw_us_v1_layer3_main_runtime_installation_plan, smw_us_v1_layer3_main_runtime_payload,
    smw_us_v1_layer3_main_runtime_verified_fixed_writes,
};
pub use layer3_runtime::{
    Layer3RuntimeBuildError, Layer3RuntimeBundle, Layer3RuntimeFragment,
    Layer3RuntimeMissingComponent, smw_us_v1_layer3_level_dispatch_fragment,
    smw_us_v1_layer3_main_dispatch_setup_fragment, smw_us_v1_layer3_main_fragment,
    smw_us_v1_layer3_mode_value_fragment, smw_us_v1_layer3_status_fragment,
    smw_us_v1_layer3_vanilla_fallback_fragment, smw_us_v1_verified_layer3_runtime_bundle,
};
pub use layer3_scroll::{
    Layer3DynamicHorizontalOutcome, Layer3DynamicHorizontalState, Layer3DynamicVerticalOutcome,
    Layer3DynamicVerticalState, Layer3ScrollFormula, smw_us_v1_layer3_horizontal_scroll,
    smw_us_v1_layer3_vertical_scroll, smw_us_v1_step_dynamic_horizontal,
    smw_us_v1_step_dynamic_vertical_accumulator, smw_us_v1_step_dynamic_vertical_camera,
};
pub use layer3_scroll_code::{
    Layer3ScrollHelperLibrary, Layer3ScrollHelperTarget, smw_us_v1_layer3_scroll_helper_library,
};
pub use lfix3_install::{
    SMW_US_V1_LFIX3_SEARCH_END, SMW_US_V1_LFIX3_SEARCH_START, smw_us_v1_lfix3_installation_plan,
};
pub use lfix3_runtime::{
    Lfix3RuntimeLengthError, SMW_US_V1_LFIX3_RUNTIME_LEN, smw_us_v1_lfix3_runtime_payload,
    smw_us_v1_lfix3_runtime_template,
};
pub use lunar_magic_metadata::{
    SMW_US_V1_LM_ATTRIBUTION_OFFSET, SMW_US_V1_LM_FEATURE_RECORD_OFFSET,
    SMW_US_V1_LM_VRAM_VERSION_OFFSET, smw_us_v1_lunar_magic_metadata_layout,
};
pub use native_map16_remap::{
    GroupedMap16RemapRecord, LoadedSmwUsV1Map16Remaps, Map16RemapRange,
    SMW_US_V1_GROUPED_MAP16_DESTINATION_POINTER_IN_RUNTIME,
    SMW_US_V1_GROUPED_MAP16_FLAGS_POINTER_IN_RUNTIME,
    SMW_US_V1_GROUPED_MAP16_OFFSETS_POINTER_IN_RUNTIME, SMW_US_V1_GROUPED_MAP16_RUNTIME_POINTER,
    SMW_US_V1_GROUPED_MAP16_SOURCE_POINTER_IN_RUNTIME, SMW_US_V1_MAP16_REMAP_GROUPS,
    SMW_US_V1_MAP16_REMAP_RANGE_OFFSETS, SMW_US_V1_MAP16_REMAP_RANGE_RECORDS_POINTER,
    SmwUsV1Map16RemapError, load_smw_us_v1_installed_map16_remaps,
};
pub use native_map16_transfer::{
    LoadedSmwUsV1TransferredMap16, SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET,
    SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET, SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET,
    SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET, SMW_US_V1_MAP16_DEFAULT_ACTS_LIKE,
    SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET, SMW_US_V1_MAP16_DEFINITION_BYTES,
    SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET, SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET,
    SMW_US_V1_MAP16_MAX_ENTRIES, SmwUsV1TransferredMap16Error, load_smw_us_v1_transferred_map16,
};
pub use overworld_boss_sequence::{
    SMW_US_V1_BOSS_SEQUENCE_FIRST_POINTER, SMW_US_V1_BOSS_SEQUENCE_SEARCH_END,
    SMW_US_V1_BOSS_SEQUENCE_SEARCH_START, smw_us_v1_boss_sequence_allocation_policy,
    smw_us_v1_boss_sequence_locator, smw_us_v1_boss_sequence_update_policy,
};
pub use overworld_event::{
    SMW_US_V1_OVERWORLD_EVENT_DESTINATION_OPERAND_OFFSET, SMW_US_V1_OVERWORLD_EVENT_FIXED_ENTRIES,
    SMW_US_V1_OVERWORLD_EVENT_SEARCH_END, SMW_US_V1_OVERWORLD_EVENT_SEARCH_START,
    SMW_US_V1_OVERWORLD_EVENT_SOURCE_OPERAND_OFFSET, smw_us_v1_overworld_event_allocation_policy,
    smw_us_v1_overworld_event_reveal_locator,
};
pub use overworld_event_number::{
    SMW_US_V1_EVENT_NUMBER_EXTENDED_MAP_OFFSET, SMW_US_V1_EVENT_NUMBER_FIXED_MAP_OFFSET,
    SMW_US_V1_EVENT_NUMBER_HOOK_OFFSET, SMW_US_V1_EVENT_NUMBER_LEGACY_PAIRS_OFFSET,
    SMW_US_V1_EVENT_NUMBER_LEGACY_PROBE_OFFSET, SMW_US_V1_EVENT_NUMBER_RUNTIME,
    SMW_US_V1_EVENT_NUMBER_RUNTIME_OFFSET, smw_us_v1_overworld_event_number_map_locator,
};
pub use overworld_event_tilemap::{
    LoadedSmwUsV1EventTilemaps, SMW_US_V1_EVENT_TILEMAP_LOADER_MARKER,
    SMW_US_V1_EVENT_TILEMAP_PRIMARY_BANK, SMW_US_V1_EVENT_TILEMAP_PRIMARY_LOW_WORD,
    SMW_US_V1_EVENT_TILEMAP_SEARCH_END, SMW_US_V1_EVENT_TILEMAP_SEARCH_START,
    SMW_US_V1_EVENT_TILEMAP_SECONDARY_BANK, SMW_US_V1_EVENT_TILEMAP_SECONDARY_LOW_WORD,
    SMW_US_V1_EVENT_TILEMAP_SECONDARY_MARKER, SmwUsV1EventTilemapLoadError,
    SmwUsV1EventTilemapStorage, load_smw_us_v1_event_tilemaps,
    smw_us_v1_event_tilemap_installation_plan, smw_us_v1_event_tilemap_locator,
    smw_us_v1_event_tilemap_update_policy,
};
pub use overworld_level_name::{
    OverworldLevelNamePatchBuildError, SMW_US_V1_OVERWORLD_NAME_CODES_OFFSET,
    SMW_US_V1_OVERWORLD_NAME_PRIMARY_HOOK_OFFSET, SMW_US_V1_OVERWORLD_NAME_RUNTIME_OFFSET,
    SMW_US_V1_OVERWORLD_NAME_SEARCH_END, SMW_US_V1_OVERWORLD_NAME_SEARCH_START,
    SMW_US_V1_OVERWORLD_NAME_SECONDARY_HOOK_OFFSET,
    SMW_US_V1_OVERWORLD_NAME_SEGMENT_OFFSETS_OFFSET, SMW_US_V1_OVERWORLD_NAME_TEXT_LEN,
    SMW_US_V1_OVERWORLD_NAME_TEXT_OFFSET, smw_us_v1_overworld_level_name_allocation_policy,
    smw_us_v1_overworld_level_name_installation_plan, smw_us_v1_overworld_level_name_locator,
    smw_us_v1_overworld_level_name_runtime,
};
pub use overworld_message_patch::{
    LoadedSmwUsV1OverworldMessages, OverworldMessagePatchBuildError,
    SMW_US_V1_OVERWORLD_MESSAGE_HOOK_EXPECTED, SMW_US_V1_OVERWORLD_MESSAGE_HOOK_OFFSET,
    SMW_US_V1_OVERWORLD_MESSAGE_POINTER_OFFSET, SMW_US_V1_OVERWORLD_MESSAGE_RUNTIME_OFFSET,
    SMW_US_V1_OVERWORLD_MESSAGE_SEARCH_END, SMW_US_V1_OVERWORLD_MESSAGE_SEARCH_START,
    SMW_US_V1_OVERWORLD_MESSAGE_SELECTOR_OFFSET, SMW_US_V1_OVERWORLD_MESSAGE_TEXT_LEN,
    SMW_US_V1_OVERWORLD_MESSAGE_TEXT_OFFSET, SmwUsV1OverworldMessageLoadError,
    SmwUsV1OverworldMessageStorage, load_smw_us_v1_overworld_messages,
    smw_us_v1_overworld_message_allocation_policy, smw_us_v1_overworld_message_installation_plan,
    smw_us_v1_overworld_message_patch_locator, smw_us_v1_overworld_message_runtime,
};
pub use overworld_path::{
    SMW_US_V1_OVERWORLD_PATH_DESTINATION_OFFSET, SMW_US_V1_OVERWORLD_PATH_LINK_COUNT,
    SMW_US_V1_OVERWORLD_PATH_SOURCE_OFFSET, SMW_US_V1_OVERWORLD_PATH_TARGET_OFFSET,
    smw_us_v1_overworld_path_link_layout,
};
pub use overworld_path_patch::{
    OverworldPathPatchBuildError, SMW_US_V1_OVERWORLD_PATH_HOOK_OFFSET,
    SMW_US_V1_OVERWORLD_PATH_PATCH_SEARCH_END, SMW_US_V1_OVERWORLD_PATH_PATCH_SEARCH_START,
    smw_us_v1_overworld_path_allocation_policy, smw_us_v1_overworld_path_installation_plan,
    smw_us_v1_overworld_path_patch_locator, smw_us_v1_overworld_path_update_policy,
};
pub use overworld_player_start::{
    SMW_US_V1_OVERWORLD_CUSTOM_START_ENABLED, SMW_US_V1_OVERWORLD_CUSTOM_START_PATCH_OFFSET,
    SMW_US_V1_OVERWORLD_CUSTOM_START_PRISTINE, SMW_US_V1_OVERWORLD_PLAYER_START_OPTIONS_OFFSET,
    smw_us_v1_overworld_player_start_layout,
};
pub use overworld_settings::{
    LoadedSmwUsV1OverworldLayer3Settings, LoadedSmwUsV1OverworldSettings,
    SMW_US_V1_EXPANDED_SETTINGS_PAYLOAD_OFFSET, SMW_US_V1_EXPANDED_SETTINGS_TABLE_OFFSET,
    SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT, SmwUsV1OverworldSettingsLoadError,
    load_smw_us_v1_overworld_layer3_settings, load_smw_us_v1_overworld_settings,
    smw_us_v1_expanded_settings_layout, smw_us_v1_overworld_layer3_settings_layout,
};
pub use overworld_special_event::{
    SMW_US_V1_SPECIAL_EVENT_DESTINATION_OPERAND, SMW_US_V1_SPECIAL_EVENT_DIRECTION_OPERAND,
    SMW_US_V1_SPECIAL_EVENT_FIXED_DESTINATION, SMW_US_V1_SPECIAL_EVENT_FIXED_DIRECTIONS,
    SMW_US_V1_SPECIAL_EVENT_FIXED_SOURCE, SMW_US_V1_SPECIAL_EVENT_SEARCH_END,
    SMW_US_V1_SPECIAL_EVENT_SEARCH_START, SMW_US_V1_SPECIAL_EVENT_SOURCE_OPERAND,
    SpecialEventRevealPatchBuildError, smw_us_v1_special_event_allocation_policy,
    smw_us_v1_special_event_reveal_installation_plan, smw_us_v1_special_event_reveal_locator,
    smw_us_v1_special_event_update_policy,
};
pub use overworld_warp::{
    SMW_US_V1_OVERWORLD_WARP_DESTINATION_HORIZONTAL_OFFSET,
    SMW_US_V1_OVERWORLD_WARP_DESTINATION_VERTICAL_OFFSET, SMW_US_V1_OVERWORLD_WARP_LINK_COUNT,
    SMW_US_V1_OVERWORLD_WARP_SOURCE_HORIZONTAL_OFFSET,
    SMW_US_V1_OVERWORLD_WARP_SOURCE_VERTICAL_OFFSET, smw_us_v1_overworld_warp_link_layout,
};
pub use overworld_warp_patch::{
    OverworldWarpPatchBuildError, SMW_US_V1_OVERWORLD_WARP_ENTRY_HOOK_OFFSET,
    SMW_US_V1_OVERWORLD_WARP_PATCH_SEARCH_END, SMW_US_V1_OVERWORLD_WARP_PATCH_SEARCH_START,
    SMW_US_V1_OVERWORLD_WARP_RETURN_HOOK_OFFSET, smw_us_v1_overworld_warp_allocation_policy,
    smw_us_v1_overworld_warp_installation_plan, smw_us_v1_overworld_warp_patch_locator,
    smw_us_v1_overworld_warp_runtime_template, smw_us_v1_overworld_warp_update_policy,
};
pub use reader::RevisionProfileReadError;
pub use revision_patch::{
    RevisionPatchPlanError, RevisionPatchTemplate, RevisionPatchTemplateError,
};
pub use secondary_exit::{
    SMW_US_V1_SECONDARY_EXIT_FIRST_READER, SMW_US_V1_SECONDARY_EXIT_FIXED_PLANES,
    SMW_US_V1_SECONDARY_EXIT_SEARCH_START, SMW_US_V1_SECONDARY_EXIT_SECOND_READER,
    smw_us_v1_secondary_exit_allocation_policy, smw_us_v1_secondary_exit_locator,
};
pub use secondary_exit_install::{
    SecondaryExitInstallBuildError, smw_us_v1_builtin_secondary_exit_installation_plan,
    smw_us_v1_secondary_exit_installation_plan,
};
pub use secondary_exit_runtime::{
    SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT, SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT_LEN,
    SMW_US_V1_SECONDARY_EXIT_FIRST_READER_LEN, SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT,
    SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT_LEN, SMW_US_V1_SECONDARY_EXIT_SECOND_READER_LEN,
    smw_us_v1_secondary_exit_first_reader, smw_us_v1_secondary_exit_second_reader,
};
pub use shared_palette::{
    SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER, SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER_OFFSET,
    SMW_US_V1_SHARED_PALETTE_OFFSET, smw_us_v1_shared_palette_layout,
};
pub use shared_palette_install::{
    SMW_US_V1_CUSTOM_PALETTE_COLORS, SMW_US_V1_CUSTOM_PALETTE_ENTRIES,
    SMW_US_V1_CUSTOM_PALETTE_POINTER_TABLE_OFFSET, SharedPaletteInstallPlanError,
    smw_us_v1_custom_palette_installation, smw_us_v1_custom_palette_layout,
    smw_us_v1_expanded_shared_palette_installation_plan,
};
pub use text::RevisionProfileError;
pub use title_recording::{
    SMW_US_V1_TITLE_RECORDING_CONTINUATION_OFFSET, SMW_US_V1_TITLE_RECORDING_HOOK_OFFSET,
    SMW_US_V1_TITLE_RECORDING_SEARCH_START, smw_us_v1_title_recording_allocation_policy,
    smw_us_v1_title_recording_locator,
};
pub use title_tilemap::{
    SMW_US_V1_TITLE_TILEMAP_POINTER_OFFSET, SMW_US_V1_TITLE_TILEMAP_PRISTINE_STREAM_OFFSET,
    SMW_US_V1_TITLE_TILEMAP_SEARCH_START, smw_us_v1_title_tilemap_allocation_policy,
    smw_us_v1_title_tilemap_locator,
};
pub use vanilla_level_map16::{
    LoadedSmwUsV1LevelMap16Base, SMW_US_V1_MAP16_BASE_BYTES, SMW_US_V1_MAP16_BASE_TILE_COUNT,
    SMW_US_V1_MAP16_COMMON_WORD_OFFSET, SMW_US_V1_MAP16_OCCUPANCY_MASK_BYTES,
    SMW_US_V1_MAP16_OCCUPANCY_MASK_OFFSET, SMW_US_V1_MAP16_SOURCE_BANK_OFFSET,
    SMW_US_V1_MAP16_TILE_BYTES, SMW_US_V1_MAP16_TILESET_WORD_TABLE_OFFSET,
    SmwUsV1LevelMap16BaseError, load_smw_us_v1_level_map16_base,
};
pub use vanilla_smw::{
    SMW_US_V1_GRAPHICS_POINTER_BANK_OFFSET, SMW_US_V1_GRAPHICS_POINTER_HIGH_OFFSET,
    SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET, SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET,
    SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_OFFSET, SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET,
    SMW_US_V1_VANILLA_GRAPHICS_FILES, SMW_US_V1_VANILLA_LEVEL_SLOTS,
    smw_us_v1_vanilla_graphics_layout, smw_us_v1_vanilla_level_layout,
};

/// Complete, externally recovered ROM-layout metadata for one supported game revision.
///
/// Profiles deliberately contain no defaults for ROM addresses. A frontend must load an audited
/// profile matching the opened ROM instead of silently guessing a layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevisionProfile {
    pub name: String,
    pub game: SupportedGame,
    pub region: Region,
    pub revision: u8,
    pub mapper: Mapper,
    pub level: LevelRomLayout,
    pub layer2: Option<LevelLayer2RomLayout>,
    pub map16: Map16RomLayout,
    pub graphics: GraphicsRomLayout,
    pub palette: PaletteRomLayout,
    pub palette_installation: InstalledLayout<PaletteRomLayout>,
    pub exanimation: ExAnimationRomLayout,
    pub exanimation_installation: InstalledLayout<InstalledExAnimationRomLayout>,
    pub expanded_settings: Option<ExpandedLevelSettingsLayout>,
    pub overworld: CompleteOverworldRomLayout,
    pub overworld_shape: CompleteOverworldShape,
    pub sprite_lengths: SpriteLengthTable,
    pub exanimation_double_size_modes: [bool; 256],
}

impl RevisionProfile {
    pub const MAGIC: &'static str = "LMREVPRO1";
    pub const MAX_TEXT_LEN: usize = 16 * 1024;
    pub const MAX_LINE_LEN: usize = 4096;
    pub const MAX_LINES: usize = 256;
    pub const MAX_NAME_LEN: usize = 256;
    pub const MAX_POINTER_TABLE_ENTRIES: usize = 4096;

    /// Parses and validates the strict, line-oriented revision profile format.
    ///
    /// # Errors
    ///
    /// Rejects malformed, duplicate, unknown, or missing fields, invalid lookup tables, mapper
    /// disagreement, unsafe dimensions, and pointer tables outside the mapper's address space.
    pub fn parse(input: &str) -> Result<Self, RevisionProfileError> {
        text::parse(input)
    }

    /// Reads at most one bounded profile document and then applies the strict text parser.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionProfileReadError`] for I/O, UTF-8, size, or profile validation failures.
    pub fn read_from(reader: impl std::io::Read) -> Result<Self, RevisionProfileReadError> {
        reader::read(reader)
    }

    /// Audits every declared pointer-table entry against one immutable ROM image.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionProfileAuditError`] for identity mismatch, unreadable tables, invalid
    /// mapped addresses, or targets outside the logical image.
    pub fn audit_rom(
        &self,
        rom: &lm_rom::RomImage,
    ) -> Result<RevisionProfileAudit, RevisionProfileAuditError> {
        audit::audit(self, rom)
    }

    /// Produces the canonical, deterministic text representation.
    #[must_use]
    pub fn encode(&self) -> String {
        text_encode::encode(self)
    }

    /// Checks the stable cartridge identity fields for this externally audited profile.
    #[must_use]
    pub fn matches_identity(&self, identity: &RomIdentity) -> bool {
        self.game == identity.game
            && self.region == identity.region
            && self.revision == identity.revision
            && self.mapper == identity.mapper
    }

    /// Validates that this profile belongs to the detected ROM family.
    ///
    /// # Errors
    ///
    /// Returns [`RevisionProfileError::IdentityMismatch`] when a stable identity field differs.
    pub fn ensure_identity(&self, identity: &RomIdentity) -> Result<(), RevisionProfileError> {
        if self.matches_identity(identity) {
            Ok(())
        } else {
            Err(RevisionProfileError::IdentityMismatch {
                actual_game: identity.game,
                actual_region: identity.region,
                actual_revision: identity.revision,
                actual_mapper: identity.mapper,
            })
        }
    }

    /// Validates a programmatically constructed profile before it is used or persisted.
    ///
    /// # Errors
    ///
    /// Returns a structured profile error for identity-independent layout and table violations.
    pub fn validate(&self) -> Result<(), RevisionProfileError> {
        if self.name.len() > Self::MAX_NAME_LEN {
            return Err(RevisionProfileError::NameTooLong {
                actual: self.name.len(),
                maximum: Self::MAX_NAME_LEN,
            });
        }
        if self.name.is_empty()
            || self.name.trim() != self.name
            || self.name.contains(['\n', '\r', '#'])
        {
            return Err(RevisionProfileError::InvalidName);
        }
        self.validate_mappers()?;
        self.validate_installations()?;
        self.validate_tables()?;
        self.validate_expanded_settings()?;
        self.validate_shapes()?;
        if self
            .sprite_lengths
            .encoded()
            .iter()
            .any(|length| *length < 3)
        {
            return Err(RevisionProfileError::InvalidSpriteLength);
        }
        Ok(())
    }

    fn validate_mappers(&self) -> Result<(), RevisionProfileError> {
        for (domain, actual) in [
            ("level", self.level.mapper),
            ("map16", self.map16.mapper),
            ("graphics", self.graphics.mapper),
            ("palette", self.palette.mapper),
            ("exanimation", self.exanimation.mapper),
            ("overworld.layers", self.overworld.layers.mapper),
            ("overworld.events", self.overworld.event_reveals.mapper),
            ("overworld.endpoints", self.overworld.endpoints.mapper),
            ("overworld.messages", self.overworld.messages.mapper),
            ("overworld.sprites", self.overworld.sprites.mapper),
            ("overworld.palette", self.overworld.palette.mapper),
            ("overworld.animation", self.overworld.animation.mapper),
        ] {
            if actual != self.mapper {
                return Err(RevisionProfileError::MapperMismatch { domain, actual });
            }
        }
        if let Some(layout) = self.layer2
            && layout.mapper != self.mapper
        {
            return Err(RevisionProfileError::MapperMismatch {
                domain: "level.layer2",
                actual: layout.mapper,
            });
        }
        if let Some(layout) = self.expanded_settings
            && layout.mapper != self.mapper
        {
            return Err(RevisionProfileError::MapperMismatch {
                domain: "expanded_settings",
                actual: layout.mapper,
            });
        }
        Ok(())
    }

    fn validate_installations(&self) -> Result<(), RevisionProfileError> {
        let validate_marker = |domain, marker: lm_project::InstallationMarker| {
            pc_to_snes(self.mapper, marker.offset)
                .map(|_| ())
                .map_err(|_| RevisionProfileError::UnmappedPointerTable(domain))
        };
        match self.palette_installation {
            lm_project::InstalledLayout::Absent => {}
            lm_project::InstalledLayout::Unconditional(layout) => {
                if layout != self.palette {
                    return Err(RevisionProfileError::InstallationLayoutMismatch("palette"));
                }
            }
            lm_project::InstalledLayout::Alternatives { primary, fallback } => {
                if primary.layout != self.palette || fallback.is_some() {
                    return Err(RevisionProfileError::InstallationLayoutMismatch("palette"));
                }
                validate_marker("palette.installation_marker", primary.marker)?;
            }
        }
        let validate_exanimation = |domain, layout: lm_project::InstalledExAnimationRomLayout| {
            if layout.payload != self.exanimation {
                return Err(RevisionProfileError::InstallationLayoutMismatch(domain));
            }
            if layout.pointer_presence_mask == 0 || layout.pointer_presence_mask & !0x00ff_ffff != 0
            {
                return Err(RevisionProfileError::InvalidPointerPresenceMask);
            }
            if let Some(locator) = layout.pointer_locator {
                if locator.mapper != self.mapper {
                    return Err(RevisionProfileError::MapperMismatch {
                        domain: "exanimation.pointer_locator",
                        actual: locator.mapper,
                    });
                }
                let final_byte = locator.first_operand_offset.checked_add(2).ok_or(
                    RevisionProfileError::AddressOverflow("exanimation.pointer_locator"),
                )?;
                for offset in [locator.first_operand_offset, final_byte] {
                    pc_to_snes(self.mapper, offset).map_err(|_| {
                        RevisionProfileError::UnmappedPointerTable("exanimation.pointer_locator")
                    })?;
                }
            }
            Ok(())
        };
        match self.exanimation_installation {
            lm_project::InstalledLayout::Absent => {}
            lm_project::InstalledLayout::Unconditional(layout) => {
                validate_exanimation("exanimation", layout)?;
            }
            lm_project::InstalledLayout::Alternatives { primary, fallback } => {
                validate_exanimation("exanimation", primary.layout)?;
                validate_marker("exanimation.installation_marker", primary.marker)?;
                if let Some(fallback) = fallback {
                    validate_exanimation("exanimation.fallback", fallback.layout)?;
                    validate_marker("exanimation.fallback_installation_marker", fallback.marker)?;
                }
            }
        }
        Ok(())
    }

    fn validate_expanded_settings(&self) -> Result<(), RevisionProfileError> {
        let Some(layout) = self.expanded_settings else {
            return Ok(());
        };
        if layout.entries == 0 || layout.stride < lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN
        {
            return Err(RevisionProfileError::InvalidExpandedSettingsLayout);
        }
        let final_offset = layout
            .entries
            .checked_sub(1)
            .and_then(|last| last.checked_mul(layout.stride))
            .and_then(|delta| layout.table_offset.checked_add(delta))
            .and_then(|offset| {
                offset.checked_add(lm_level::ExpandedLevelSettingsRecord::ENCODED_LEN - 1)
            })
            .ok_or(RevisionProfileError::AddressOverflow("expanded_settings"))?;
        pc_to_snes(layout.mapper, final_offset)
            .map_err(|_| RevisionProfileError::UnmappedPointerTable("expanded_settings"))?;
        let span = layout.table_offset..final_offset + 1;
        let mut tables = vec![
            ("level.layer1", self.level.layer1),
            (
                "level.sprites",
                self.level.sprites.low_or_contiguous_table(),
            ),
            ("map16.graphics", self.map16.graphics),
            ("map16.acts_like", self.map16.acts_like),
            ("graphics", self.graphics.pointers),
            ("palette", self.palette.pointers),
            ("exanimation", self.exanimation.pointers),
            ("overworld.layer1", self.overworld.layers.layer1),
            ("overworld.layer2", self.overworld.layers.layer2),
            (
                "overworld.event_sources",
                self.overworld.event_reveals.sources,
            ),
            (
                "overworld.event_destinations",
                self.overworld.event_reveals.destinations,
            ),
            ("overworld.endpoints", self.overworld.endpoints.pointers),
            ("overworld.messages", self.overworld.messages.pointers),
            ("overworld.sprites", self.overworld.sprites.pointers),
            ("overworld.palette", self.overworld.palette.pointers),
            ("overworld.animation", self.overworld.animation.pointers),
        ];
        if let Some(layer2) = self.layer2 {
            tables.push(("level.layer2", layer2.pointers));
        }
        for (domain, table) in tables {
            let pointer_span = table_span(domain, table)?;
            if span.start < pointer_span.end && pointer_span.start < span.end {
                return Err(RevisionProfileError::ExpandedSettingsTableOverlap {
                    pointer_table: domain,
                });
            }
        }
        let sprite_bank_spans = match self.level.sprites {
            lm_project::SpritePointerTable::Contiguous(_) => Vec::new(),
            lm_project::SpritePointerTable::SplitSharedBank { bank_offset, .. } => {
                let end = bank_offset
                    .checked_add(1)
                    .ok_or(RevisionProfileError::AddressOverflow("level.sprites.bank"))?;
                vec![("level.sprites.bank", bank_offset..end)]
            }
            lm_project::SpritePointerTable::SplitBankTable { banks, .. } => vec![(
                "level.sprites.banks",
                component_span("level.sprites.banks", banks, 1)?,
            )],
        };
        for (domain, pointer_span) in sprite_bank_spans {
            if span.start < pointer_span.end && pointer_span.start < span.end {
                return Err(RevisionProfileError::ExpandedSettingsTableOverlap {
                    pointer_table: domain,
                });
            }
        }
        Ok(())
    }

    fn validate_tables(&self) -> Result<(), RevisionProfileError> {
        let mut tables = vec![
            ("level.layer1", self.level.layer1),
            (
                "level.sprites",
                self.level.sprites.low_or_contiguous_table(),
            ),
            ("map16.graphics", self.map16.graphics),
            ("map16.acts_like", self.map16.acts_like),
            ("graphics", self.graphics.pointers),
            ("palette", self.palette.pointers),
            ("exanimation", self.exanimation.pointers),
            ("overworld.layer1", self.overworld.layers.layer1),
            ("overworld.layer2", self.overworld.layers.layer2),
            (
                "overworld.event_sources",
                self.overworld.event_reveals.sources,
            ),
            (
                "overworld.event_destinations",
                self.overworld.event_reveals.destinations,
            ),
            ("overworld.endpoints", self.overworld.endpoints.pointers),
            ("overworld.messages", self.overworld.messages.pointers),
            ("overworld.sprites", self.overworld.sprites.pointers),
            ("overworld.palette", self.overworld.palette.pointers),
            ("overworld.animation", self.overworld.animation.pointers),
        ];
        if let Some(layer2) = self.layer2 {
            tables.push(("level.layer2", layer2.pointers));
        }
        for (domain, table) in tables.iter().copied() {
            if table.entries > Self::MAX_POINTER_TABLE_ENTRIES {
                return Err(RevisionProfileError::PointerTableEntryLimit {
                    domain,
                    actual: table.entries,
                    maximum: Self::MAX_POINTER_TABLE_ENTRIES,
                });
            }
            if domain != "level.sprites" {
                validate_table(self.mapper, domain, table)?;
            }
        }
        let mut spans = tables
            .iter()
            .filter(|(domain, _)| *domain != "level.sprites")
            .map(|(domain, table)| Ok((*domain, table_span(domain, *table)?)))
            .collect::<Result<Vec<_>, RevisionProfileError>>()?;
        match self.level.sprites {
            lm_project::SpritePointerTable::Contiguous(table) => {
                validate_table(self.mapper, "level.sprites", table)?;
                spans.push(("level.sprites", table_span("level.sprites", table)?));
            }
            lm_project::SpritePointerTable::SplitSharedBank {
                low_words,
                bank_offset,
            } => {
                validate_component(self.mapper, "level.sprites", low_words, 2)?;
                validate_direct_byte(self.mapper, "level.sprites.bank", bank_offset)?;
                spans.push((
                    "level.sprites",
                    component_span("level.sprites", low_words, 2)?,
                ));
                let end = bank_offset
                    .checked_add(1)
                    .ok_or(RevisionProfileError::AddressOverflow("level.sprites.bank"))?;
                spans.push(("level.sprites.bank", bank_offset..end));
            }
            lm_project::SpritePointerTable::SplitBankTable { low_words, banks } => {
                validate_component(self.mapper, "level.sprites", low_words, 2)?;
                validate_component(self.mapper, "level.sprites.banks", banks, 1)?;
                if low_words.entries != banks.entries {
                    return Err(RevisionProfileError::IncompleteSpritePointerLayout);
                }
                spans.push((
                    "level.sprites",
                    component_span("level.sprites", low_words, 2)?,
                ));
                spans.push((
                    "level.sprites.banks",
                    component_span("level.sprites.banks", banks, 1)?,
                ));
            }
        }
        for first in 0..spans.len() {
            let first_span = &spans[first].1;
            for second in first + 1..spans.len() {
                let second_span = &spans[second].1;
                if first_span.start < second_span.end && second_span.start < first_span.end {
                    return Err(RevisionProfileError::OverlappingPointerTables {
                        first: spans[first].0,
                        second: spans[second].0,
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_shapes(&self) -> Result<(), RevisionProfileError> {
        if self
            .layer2
            .is_some_and(|layout| layout.maximum_compressed_len == 0)
        {
            return Err(RevisionProfileError::ZeroValue(
                "level.layer2.maximum_compressed_len",
            ));
        }
        let positive = [
            (
                "graphics.maximum_compressed_len",
                self.graphics.maximum_compressed_len,
            ),
            (
                "graphics.maximum_decompressed_len",
                self.graphics.maximum_decompressed_len,
            ),
            ("palette.colors", self.palette.colors_per_palette),
            (
                "exanimation.maximum_records",
                self.exanimation.maximum_records,
            ),
            (
                "exanimation.maximum_encoded_len",
                self.exanimation.maximum_encoded_len,
            ),
            ("overworld.width", self.overworld_shape.width),
            ("overworld.height", self.overworld_shape.height),
            (
                "overworld.event_reveals",
                self.overworld_shape.event_reveals,
            ),
            ("overworld.endpoints", self.overworld_shape.endpoints),
            ("overworld.messages", self.overworld_shape.messages),
            ("overworld.sprites", self.overworld_shape.sprites),
            (
                "overworld.sprite_record_len",
                self.overworld_shape.sprite_record_len,
            ),
            (
                "overworld.palette_colors",
                self.overworld_shape.palette_colors,
            ),
        ];
        for (field, value) in positive {
            if value == 0 {
                return Err(RevisionProfileError::ZeroValue(field));
            }
        }
        if self.overworld.layers.width != self.overworld_shape.width
            || self.overworld.layers.height != self.overworld_shape.height
            || self.overworld.event_reveals.entries_per_slot != self.overworld_shape.event_reveals
            || self.overworld.endpoints.endpoints_per_slot != self.overworld_shape.endpoints
            || self.overworld.messages.messages_per_slot != self.overworld_shape.messages
            || self.overworld.sprites.sprites_per_slot != self.overworld_shape.sprites
            || self.overworld.sprites.record_len != self.overworld_shape.sprite_record_len
            || self.overworld.palette.colors_per_palette != self.overworld_shape.palette_colors
        {
            return Err(RevisionProfileError::OverworldShapeMismatch);
        }
        Ok(())
    }
}

fn validate_component(
    mapper: Mapper,
    domain: &'static str,
    table: LevelPointerTable,
    width: usize,
) -> Result<(), RevisionProfileError> {
    if table.entries == 0 {
        return Err(RevisionProfileError::ZeroValue(domain));
    }
    if table.stride < width {
        return Err(RevisionProfileError::InvalidPointerStride {
            domain,
            stride: table.stride,
        });
    }
    let span = component_span(domain, table, width)?;
    pc_to_snes(mapper, span.end - 1)
        .map_err(|_| RevisionProfileError::UnmappedPointerTable(domain))?;
    Ok(())
}

fn validate_direct_byte(
    mapper: Mapper,
    domain: &'static str,
    offset: usize,
) -> Result<(), RevisionProfileError> {
    pc_to_snes(mapper, offset)
        .map(|_| ())
        .map_err(|_| RevisionProfileError::UnmappedPointerTable(domain))
}

fn component_span(
    domain: &'static str,
    table: LevelPointerTable,
    width: usize,
) -> Result<Range<usize>, RevisionProfileError> {
    let len = table
        .entries
        .checked_sub(1)
        .and_then(|last| last.checked_mul(table.stride))
        .and_then(|last| last.checked_add(width))
        .ok_or(RevisionProfileError::AddressOverflow(domain))?;
    let end = table
        .offset
        .checked_add(len)
        .ok_or(RevisionProfileError::AddressOverflow(domain))?;
    Ok(table.offset..end)
}

fn validate_table(
    mapper: Mapper,
    domain: &'static str,
    table: LevelPointerTable,
) -> Result<(), RevisionProfileError> {
    if table.entries == 0 {
        return Err(RevisionProfileError::ZeroValue(domain));
    }
    if table.stride < 3 {
        return Err(RevisionProfileError::InvalidPointerStride {
            domain,
            stride: table.stride,
        });
    }
    let final_offset = table
        .entries
        .checked_sub(1)
        .and_then(|last| last.checked_mul(table.stride))
        .and_then(|delta| table.offset.checked_add(delta))
        .and_then(|offset| offset.checked_add(2))
        .ok_or(RevisionProfileError::AddressOverflow(domain))?;
    pc_to_snes(mapper, final_offset)
        .map_err(|_| RevisionProfileError::UnmappedPointerTable(domain))?;
    Ok(())
}

fn table_span(
    domain: &'static str,
    table: LevelPointerTable,
) -> Result<Range<usize>, RevisionProfileError> {
    let len = table
        .entries
        .checked_sub(1)
        .and_then(|last| last.checked_mul(table.stride))
        .and_then(|last| last.checked_add(3))
        .ok_or(RevisionProfileError::AddressOverflow(domain))?;
    let end = table
        .offset
        .checked_add(len)
        .ok_or(RevisionProfileError::AddressOverflow(domain))?;
    Ok(table.offset..end)
}

impl fmt::Display for RevisionProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.encode())
    }
}

#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub mod test_support;
