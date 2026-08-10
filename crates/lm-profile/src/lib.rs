//! Strict, identity-bound external revision metadata shared by editor frontends.

mod allocation;
mod audit;
mod copier_header;
mod credits_tilemap;
mod exanimation_legacy_hooks;
mod exanimation_runtime;
mod exanimation_runtime_install;
mod expanded_settings_allocation;
mod expanded_settings_base;
mod expanded_settings_hooks;
mod expanded_settings_install;
mod expanded_settings_runtime;
mod expanded_settings_runtime_21c;
mod expanded_settings_runtime_bundle;
mod graphics_compression_runtime;
mod layer2_runtime_install;
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
mod level_access_restriction;
mod lfix3_install;
mod lfix3_runtime;
mod lunar_magic_metadata;
mod map16_runtime_install;
mod native_assets;
mod native_custom_overworld_sprite;
mod native_map16_complete;
mod native_map16_primary;
mod native_map16_remap;
mod native_map16_secondary;
mod native_map16_transfer;
mod overworld_animation_runtime;
mod overworld_boss_sequence;
mod overworld_builtin_animation;
mod overworld_builtin_lightning;
mod overworld_event;
mod overworld_event_number;
mod overworld_event_tilemap;
mod overworld_level_name;
mod overworld_main_layer2;
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
mod separate_midway_install;
mod shared_palette;
mod shared_palette_install;
mod smw_us_v1_exgraphics;
mod sprite19_fix;
mod support_patch_b;
mod text;
mod text_encode;
mod text_schema;
mod title_recording;
mod title_recording_recorder;
mod title_tilemap;
mod vanilla_layer3;
mod vanilla_level_map16;
mod vanilla_level_mode;
mod vanilla_level_palette;
mod vanilla_object_family;
mod vanilla_smw;
mod vanilla_standard_object_map;

use lm_level::SpriteLengthTable;
use lm_project::{
    CompleteOverworldRomLayout, CompleteOverworldShape, ExAnimationRomLayout,
    ExpandedLevelSettingsLayout, GraphicsRomLayout, InstalledExAnimationRomLayout, InstalledLayout,
    LevelLayer2RomLayout, LevelPointerTable, LevelRomLayout, Map16RomLayout, PaletteRomLayout,
};
use lm_rom::{Mapper, Region, RomError, RomIdentity, RomImage, SupportedGame, pc_to_snes};
use std::fmt;
use std::ops::Range;

pub use allocation::RevisionAllocationError;
pub use audit::{
    DirectTableAudit, PointerTableAudit, RevisionProfileAudit, RevisionProfileAuditError,
};
pub use copier_header::{lunar_magic_copier_header, smw_us_v1_lunar_magic_copier_header};
pub use credits_tilemap::{
    SMW_US_V1_CREDITS_BLANK_WORD, SMW_US_V1_CREDITS_EXPANDED_OFFSETS_OFFSET,
    SMW_US_V1_CREDITS_LEGACY_ROWS, SMW_US_V1_CREDITS_OFFSETS_OFFSET,
    SMW_US_V1_CREDITS_RECORDS_OFFSET, SMW_US_V1_CREDITS_RUNTIME_OFFSET,
    SMW_US_V1_CREDITS_SEARCH_START, smw_us_v1_credits_allocation_policy,
    smw_us_v1_credits_tilemap_locator, smw_us_v1_legacy_credits_tilemap_layout,
};
pub use exanimation_legacy_hooks::{
    SmwUsV1LegacyExAnimationHookMigration, SmwUsV1LegacyExAnimationHookMigrationError,
    smw_us_v1_legacy_exanimation_hook_migration,
};
pub use exanimation_runtime::{
    EXPANDED_EXANIMATION_POINTER_TABLE_LEN, EXPANDED_EXANIMATION_RUNTIME_CORE_LEN,
    EXPANDED_EXANIMATION_RUNTIME_OPTIONAL_LEN, ExpandedExAnimationRuntimeError,
    ExpandedExAnimationRuntimeOptionalRelocations, ExpandedExAnimationRuntimeRelocations,
    OPTIONAL_MAPPING_HELPER_POINTER_OFFSET, OPTIONAL_MAPPING_HELPER_SNES_ADDRESS,
    OPTIONAL_SUFFIX_POINTER_OFFSET, empty_expanded_exanimation_pointer_table,
    expanded_exanimation_runtime_optional_suffix, expanded_exanimation_runtime_template,
    expanded_exanimation_runtime_template_with_optional_suffix,
    relocate_expanded_exanimation_mapper_iram, relocate_expanded_exanimation_runtime,
    relocate_expanded_exanimation_runtime_with_optional_suffix,
};
pub use exanimation_runtime_install::{
    SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_END,
    SMW_US_V1_EXPANDED_EXANIMATION_CORE_SEARCH_START, SmwUsV1ExpandedExAnimationRuntimeDetectError,
    SmwUsV1ExpandedExAnimationRuntimeGeneration, SmwUsV1LegacyGlobalExAnimationRuntime,
    detect_smw_us_v1_current_expanded_exanimation_runtime,
    detect_smw_us_v1_current_expanded_exanimation_runtime_for_mapper,
    detect_smw_us_v1_legacy_global_exanimation_runtime,
    probe_smw_us_v1_expanded_exanimation_runtime_generation,
    probe_smw_us_v1_expanded_exanimation_runtime_generation_for_mapper,
    smw_us_v1_expanded_exanimation_core_installation_plan,
    smw_us_v1_expanded_exanimation_runtime_installation_plan,
    smw_us_v1_expanded_exanimation_runtime_installation_plan_for_mapper,
    smw_us_v1_expanded_exanimation_runtime_payload,
    smw_us_v1_expanded_exanimation_uses_mapper_runtime,
};
pub use expanded_settings_allocation::{
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN, SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT, SMW_US_V1_EXPANDED_SETTINGS_SPECIAL_RECORD_OFFSET,
    SMW_US_V1_EXPANDED_SETTINGS_STANDARD_LEVEL_COUNT, SmwUsV1ExpandedSettingsAllocation,
    SmwUsV1ExpandedSettingsAllocationError, SmwUsV1ExpandedSettingsRecordGeneration,
    smw_us_v1_default_expanded_settings_record, smw_us_v1_default_special_expanded_settings_record,
    smw_us_v1_normalize_expanded_settings_record, smw_us_v1_normalize_expanded_settings_references,
    smw_us_v1_upgrade_expanded_settings_record,
    smw_us_v1_upgrade_legacy_expanded_settings_record_layout,
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
    SMW_US_V1_EXPANDED_SETTINGS_GENERATION_101_ALLOCATION_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_GENERATION_102_MARKER,
    SMW_US_V1_EXPANDED_SETTINGS_MAXIMUM_LOROM_LEN, SMW_US_V1_GFX_EXPANDED_SETTINGS_ALLOCATION_END,
    SMW_US_V1_GFX_EXPANDED_SETTINGS_ALLOCATION_START,
    SMW_US_V1_LEGACY_GRAPHICS_GENERATION_100_MARKER,
    SMW_US_V1_LEGACY_GRAPHICS_GENERATION_100_MARKER_OFFSET,
    SMW_US_V1_LEGACY_GRAPHICS_GENERATION_101_MARKER,
    SMW_US_V1_LEGACY_GRAPHICS_GENERATION_101_MARKER_OFFSET,
    SmwUsV1ExpandedSettingsGeneration100Migration,
    SmwUsV1ExpandedSettingsGeneration100MigrationError,
    SmwUsV1ExpandedSettingsGeneration101Migration,
    SmwUsV1ExpandedSettingsGeneration101MigrationError,
    SmwUsV1ExpandedSettingsGeneration102Migration,
    SmwUsV1ExpandedSettingsGeneration102MigrationError,
    smw_us_v1_expanded_settings_generation_100_migration,
    smw_us_v1_expanded_settings_generation_101_migration,
    smw_us_v1_expanded_settings_generation_102_migration,
    smw_us_v1_expanded_settings_installation_plan,
    smw_us_v1_expanded_settings_installation_plan_for_rom,
    smw_us_v1_expanded_settings_installation_plan_for_rom_with_overworld_settings,
    smw_us_v1_expanded_settings_installation_plan_with_overworld_settings,
    smw_us_v1_gfx_expanded_settings_installation_plan,
    smw_us_v1_sa1_expanded_settings_installation_plan,
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
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_DESTINATIONS, SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER,
    SMW_US_V1_EXPANDED_SETTINGS_RUNTIME_MARKER_OFFSET,
    resolve_expanded_settings_runtime_allocation, smw_us_v1_expanded_settings_fixed_writes,
    smw_us_v1_expanded_settings_runtime_bundle, smw_us_v1_expanded_settings_runtime_writes,
};
pub use graphics_compression_runtime::{
    SMW_US_V1_GRAPHICS_COMPRESSION_HOOK_OFFSET, SMW_US_V1_GRAPHICS_COMPRESSION_METADATA_OFFSET,
    SmwUsV1GraphicsCompressionDetectError, SmwUsV1GraphicsCompressionMigrationError,
    SmwUsV1GraphicsCompressionMode, SmwUsV1GraphicsCompressionReplacementPlan,
    detect_smw_us_v1_graphics_compression_mode,
    smw_us_v1_compact_graphics_compression_migration_plan,
    smw_us_v1_lz2_original_installation_plan, smw_us_v1_lz2_speed_installation_plan,
    smw_us_v1_lz2_speed_migration_plan, smw_us_v1_lz3_installation_plan,
};
pub use layer2_runtime_install::{
    SmwUsV1Layer2Format102MigrationError, smw_us_v1_layer2_format_100_migration,
    smw_us_v1_layer2_format_101_migration, smw_us_v1_layer2_format_102_migration,
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
pub use level_access_restriction::{
    smw_us_v1_exlorom_level_access_restriction_layout, smw_us_v1_level_access_restriction_layout,
};
pub use lfix3_install::{
    SMW_US_V1_LFIX3_SEARCH_END, SMW_US_V1_LFIX3_SEARCH_START, SmwUsV1Lfix3DetectError,
    SmwUsV1Lfix3Generation, SmwUsV1Lfix3Generation1MigrationBuildError,
    SmwUsV1Lfix3Generation2Migration, SmwUsV1Lfix3Generation2MigrationBuildError,
    SmwUsV1Lfix3GenerationError, detect_smw_us_v1_current_lfix3_runtime,
    detect_smw_us_v1_generation_1_lfix3_runtime, detect_smw_us_v1_generation_2_lfix3_runtime,
    migrate_smw_us_v1_generation_1_lfix3_tables, probe_smw_us_v1_lfix3_generation,
    smw_us_v1_builtin_lfix3_installation_plan, smw_us_v1_generation_1_lfix3_migration,
    smw_us_v1_generation_2_lfix3_migration, smw_us_v1_lfix3_installation_plan,
};
pub use lfix3_runtime::{
    Lfix3RuntimeLengthError, SMW_US_V1_LFIX3_RUNTIME_LEN, smw_us_v1_lfix3_runtime_payload,
    smw_us_v1_lfix3_runtime_template,
};
pub use lunar_magic_metadata::{
    SMW_US_V1_LM_ATTRIBUTION_OFFSET, SMW_US_V1_LM_FEATURE_RECORD_OFFSET,
    SMW_US_V1_LM_VRAM_VERSION_OFFSET, smw_us_v1_lunar_magic_metadata_layout,
};
pub use map16_runtime_install::{
    SmwUsV1Map16LegacyMigrationBuildError, SmwUsV1Map16RuntimeDetectError,
    SmwUsV1Map16RuntimeGeneration, SmwUsV1Map16RuntimeInstallBuildError,
    SmwUsV1Map16StageThreeMigrationBuildError, detect_smw_us_v1_current_map16_runtime,
    detect_smw_us_v1_stage_one_map16_runtime, detect_smw_us_v1_stage_three_map16_runtime,
    detect_smw_us_v1_stage_two_map16_runtime, probe_smw_us_v1_map16_runtime_generation,
    smw_us_v1_builtin_map16_runtime_installation_plan, smw_us_v1_legacy_map16_runtime_migration,
    smw_us_v1_map16_runtime_installation_plan, smw_us_v1_stage_three_map16_runtime_migration,
};
pub use native_custom_overworld_sprite::{
    LUNAR_MAGIC_CUSTOM_OVERWORLD_SPRITE_DESCRIPTOR_FIELD,
    LUNAR_MAGIC_OVERWORLD_SPRITE_SIZE_DESCRIPTOR_FIELD,
    SMW_US_V1_CUSTOM_OVERWORLD_SPRITE_MAX_PAYLOAD_LEN, SmwUsV1NativeCustomOverworldSpriteLayout,
    SmwUsV1NativeCustomOverworldSpriteLayoutError, smw_us_v1_native_custom_overworld_sprite_layout,
};
pub use native_map16_complete::{
    LoadedSmwUsV1CompleteMap16, SavedSmwUsV1CompleteMap16, SmwUsV1CompleteMap16Error,
    SmwUsV1CompleteMap16SaveOptions, load_smw_us_v1_complete_map16, save_smw_us_v1_complete_map16,
};
pub use native_map16_primary::{
    LoadedSmwUsV1PrimaryMap16, SMW_US_V1_PRIMARY_MAP16_ACTS_LIKE_WORDS,
    SMW_US_V1_PRIMARY_MAP16_AUXILIARY_BYTES, SMW_US_V1_PRIMARY_MAP16_BLOCK_BYTES,
    SMW_US_V1_PRIMARY_MAP16_BLOCK_COUNT, SMW_US_V1_PRIMARY_MAP16_DEFINITION_WORDS,
    SMW_US_V1_PRIMARY_MAP16_FIRST_AUXILIARY_POINTER_OFFSET,
    SMW_US_V1_PRIMARY_MAP16_LEGACY_PREFIX_BYTES, SMW_US_V1_PRIMARY_MAP16_RUNTIME_BASE,
    SMW_US_V1_PRIMARY_MAP16_RUNTIME_MARKER_OFFSET,
    SMW_US_V1_PRIMARY_MAP16_SECOND_AUXILIARY_POINTER_OFFSET, SavedSmwUsV1PrimaryMap16,
    SmwUsV1PrimaryMap16Error, SmwUsV1PrimaryMap16SaveOptions, load_smw_us_v1_primary_map16,
    save_smw_us_v1_primary_map16,
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
pub use native_map16_secondary::{
    LoadedSmwUsV1SecondaryMap16, SMW_US_V1_SECONDARY_MAP16_BLOCK_BYTES,
    SMW_US_V1_SECONDARY_MAP16_BLOCK_COUNT, SMW_US_V1_SECONDARY_MAP16_DEFINITION_WORDS,
    SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_BYTES, SMW_US_V1_SECONDARY_MAP16_FIXED_BLOCK_OFFSET,
    SMW_US_V1_SECONDARY_MAP16_POINTER_TABLE_OFFSET,
    SMW_US_V1_SECONDARY_MAP16_RUNTIME_MARKER_OFFSET, SavedSmwUsV1SecondaryMap16,
    SmwUsV1SecondaryMap16Error, SmwUsV1SecondaryMap16SaveOptions, load_smw_us_v1_secondary_map16,
    save_smw_us_v1_secondary_map16,
};
pub use native_map16_transfer::{
    LoadedSmwUsV1TransferredMap16, SMW_US_V1_MAP16_ACTS_HIGH_BANK_OFFSET,
    SMW_US_V1_MAP16_ACTS_HIGH_WORD_OFFSET, SMW_US_V1_MAP16_ACTS_LOW_BANK_OFFSET,
    SMW_US_V1_MAP16_ACTS_LOW_WORD_OFFSET, SMW_US_V1_MAP16_DEFAULT_ACTS_LIKE,
    SMW_US_V1_MAP16_DEFINITION_BANK_OFFSET, SMW_US_V1_MAP16_DEFINITION_BYTES,
    SMW_US_V1_MAP16_DEFINITION_ODD_WORD_OFFSET, SMW_US_V1_MAP16_DEFINITION_WORD_OFFSET,
    SMW_US_V1_MAP16_MAX_ENTRIES, SavedSmwUsV1TransferredMap16, SmwUsV1TransferredMap16Error,
    SmwUsV1TransferredMap16SaveOptions, load_smw_us_v1_transferred_map16,
    save_smw_us_v1_transferred_map16,
};
pub use overworld_animation_runtime::{
    SMW_US_V1_OVERWORLD_ANIMATION_AUXILIARY_LEN, SMW_US_V1_OVERWORLD_ANIMATION_OPTIONS_LEN,
    SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_LEN, SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_END,
    SMW_US_V1_OVERWORLD_ANIMATION_SEARCH_START, SmwUsV1OverworldAnimationRuntime,
    SmwUsV1OverworldAnimationRuntimeError, detect_smw_us_v1_overworld_animation_runtime,
    smw_us_v1_overworld_animation_runtime_installation_plan,
    smw_us_v1_overworld_animation_runtime_template,
};
pub use overworld_boss_sequence::{
    SMW_US_V1_BOSS_SEQUENCE_FIRST_POINTER, SMW_US_V1_BOSS_SEQUENCE_SEARCH_END,
    SMW_US_V1_BOSS_SEQUENCE_SEARCH_START, smw_us_v1_boss_sequence_allocation_policy,
    smw_us_v1_boss_sequence_locator, smw_us_v1_boss_sequence_update_policy,
};
pub use overworld_builtin_animation::{
    ALL_STARS_WORLD_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET,
    LUNAR_MAGIC_OVERWORLD_ANIMATION_DESCRIPTOR_FIELD,
    SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_PHYSICAL_OFFSET,
    SMW_US_V1_BUILT_IN_OVERWORLD_ANIMATION_WORDS, SmwUsV1BuiltInOverworldAnimationError,
    SmwUsV1BuiltInOverworldAnimationTable, builtin_overworld_animation_table_offset,
    load_builtin_overworld_animation_table,
};
pub use overworld_builtin_lightning::{
    BUILT_IN_OVERWORLD_LIGHTNING_SELECTOR_LEN, BuiltInOverworldLightningLayout,
    BuiltInOverworldLightningSources, LUNAR_MAGIC_OVERWORLD_LIGHTNING_DELAYS_DESCRIPTOR_FIELD,
    LUNAR_MAGIC_OVERWORLD_LIGHTNING_MASK_DESCRIPTOR_FIELD, builtin_overworld_lightning_layout,
    probe_builtin_overworld_lightning_sources,
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
    load_smw_us_v1_event_tilemaps_for_mapper, smw_us_v1_event_tilemap_installation_plan,
    smw_us_v1_event_tilemap_locator, smw_us_v1_event_tilemap_locator_for_mapper,
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
pub use overworld_main_layer2::{
    LoadedSmwUsV1MainOverworldLayer2, SMW_US_V1_MAIN_OVERWORLD_LAYER2_BANK,
    SMW_US_V1_MAIN_OVERWORLD_LAYER2_BYTES, SMW_US_V1_MAIN_OVERWORLD_LAYER2_HEIGHT,
    SMW_US_V1_MAIN_OVERWORLD_LAYER2_HIGH_WORD, SMW_US_V1_MAIN_OVERWORLD_LAYER2_LOW_WORD,
    SMW_US_V1_MAIN_OVERWORLD_LAYER2_PRISTINE_HIGH, SMW_US_V1_MAIN_OVERWORLD_LAYER2_PRISTINE_LOW,
    SMW_US_V1_MAIN_OVERWORLD_LAYER2_WIDTH, SMW_US_V1_OVERWORLD_LAYER2_PLANE_HEIGHT,
    SMW_US_V1_OVERWORLD_LAYER2_PLANE_WIDTH, SmwUsV1MainOverworldLayer2Error,
    SmwUsV1MainOverworldLayer2SaveOptions, SmwUsV1MainOverworldLayer2Storage,
    load_smw_us_v1_main_overworld_layer2, save_smw_us_v1_main_overworld_layer2,
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
    LoadedSmwUsV1ExpandedLevelSettings, LoadedSmwUsV1OverworldLayer3Settings,
    LoadedSmwUsV1OverworldSettings, SMW_US_V1_EXPANDED_SETTINGS_PAYLOAD_OFFSET,
    SMW_US_V1_EXPANDED_SETTINGS_TABLE_OFFSET,
    SMW_US_V1_OVERWORLD_ANIMATION_FEATURE_OPERAND_DISPLACEMENT,
    SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_MARKER, SMW_US_V1_OVERWORLD_ANIMATION_RUNTIME_OPERAND,
    SMW_US_V1_OVERWORLD_LIGHTNING_DISABLE_MASK, SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
    SmwUsV1OverworldSettingsLoadError, load_smw_us_v1_expanded_level_settings,
    load_smw_us_v1_overworld_layer3_settings, load_smw_us_v1_overworld_settings,
    smw_us_v1_expanded_settings_layout, smw_us_v1_installed_expanded_settings_layout,
    smw_us_v1_overworld_animation_options_layout, smw_us_v1_overworld_layer3_settings_layout,
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
    smw_us_v1_builtin_secondary_exit_installation_plan_from_source,
    smw_us_v1_secondary_exit_installation_plan,
    smw_us_v1_secondary_exit_installation_plan_from_source,
};
pub use secondary_exit_runtime::{
    SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT, SMW_US_V1_SECONDARY_EXIT_BASE_SUPPORT_LEN,
    SMW_US_V1_SECONDARY_EXIT_FIRST_READER_LEN, SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT,
    SMW_US_V1_SECONDARY_EXIT_INDEX_SUPPORT_LEN, SMW_US_V1_SECONDARY_EXIT_SECOND_READER_LEN,
    smw_us_v1_secondary_exit_first_reader, smw_us_v1_secondary_exit_second_reader,
};
pub use separate_midway_install::{
    SeparateMidwayInstallBuildError, smw_us_v1_separate_midway_installation_plan,
};
pub use shared_palette::{
    SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER, SMW_US_V1_SHARED_PALETTE_EXPANDED_MARKER_OFFSET,
    SMW_US_V1_SHARED_PALETTE_OFFSET, smw_us_v1_shared_palette_layout,
    smw_us_v1_shared_palette_layout_for_mapper,
};
pub use shared_palette_install::{
    SMW_US_V1_CUSTOM_PALETTE_COLORS, SMW_US_V1_CUSTOM_PALETTE_ENTRIES,
    SMW_US_V1_CUSTOM_PALETTE_POINTER_TABLE_OFFSET, SharedPaletteInstallPlanError,
    smw_us_v1_custom_palette_installation, smw_us_v1_custom_palette_installation_for_mapper,
    smw_us_v1_custom_palette_layout, smw_us_v1_custom_palette_layout_for_mapper,
    smw_us_v1_expanded_shared_palette_installation_plan,
    smw_us_v1_expanded_shared_palette_installation_plan_for_mapper,
};
pub use smw_us_v1_exgraphics::{
    SMW_US_V1_4BPP_GRAPHICS_MARKER, SMW_US_V1_4BPP_GRAPHICS_MARKER_OFFSETS,
    SMW_US_V1_EXGFX_EXPANSION_MARKER, SMW_US_V1_EXGFX_EXPANSION_MARKER_OFFSET,
    SMW_US_V1_EXGFX_LOGICAL_LEN, SMW_US_V1_EXGFX_RUNTIME_HOOK, SMW_US_V1_EXGFX_RUNTIME_HOOK_OFFSET,
    SMW_US_V1_EXGFX_TABLE_BASE_OPERAND, SMW_US_V1_EXGFX_TABLE_BASE_OPERAND_OFFSET,
    SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER, SMW_US_V1_EXPANDED_GRAPHICS_FORMAT_MARKER_OFFSET,
    SMW_US_V1_EXTENDED_EXGFX_POINTER_OFFSET, SMW_US_V1_ORDINARY_EXGFX_POINTER_OFFSET,
    SMW_US_V1_RESERVED_EXGFX_MARKER, SMW_US_V1_RESERVED_EXGFX_MARKER_OFFSET,
    SMW_US_V1_RESERVED_EXGFX_POINTER_OFFSET, SMW_US_V1_VANILLA_GRAPHICS_FORMAT_MARKER,
    SmwUsV1ExGraphicsEncoding, SmwUsV1ExGraphicsError, SmwUsV1ExGraphicsPointer,
    SmwUsV1ExGraphicsRuntimeState, has_smw_us_v1_4bpp_graphics_prerequisite,
    probe_smw_us_v1_exgraphics_runtime, probe_smw_us_v1_exgraphics_runtime_for_mapper,
    requires_smw_us_v1_4bpp_graphics_warning, smw_us_v1_exgraphics_installation_plan,
    smw_us_v1_exgraphics_installation_plan_for_mapper, smw_us_v1_exgraphics_pointer,
    smw_us_v1_exgraphics_pointer_for_mapper, smw_us_v1_exgraphics_pointer_in_rom,
    smw_us_v1_sa1_exgraphics_runtime_installation_plan,
};
pub use sprite19_fix::{
    SMW_US_V1_SPRITE19_FIX_BRANCH_OFFSET, SMW_US_V1_SPRITE19_FIX_HOOK_OFFSET,
    SMW_US_V1_SPRITE19_FIX_RUNTIME_OFFSET, SmwUsV1Sprite19FixDetectError,
    SmwUsV1Sprite19FixInstallError, SmwUsV1Sprite19FixState, detect_smw_us_v1_sprite19_fix,
    smw_us_v1_sprite19_fix_installation_plan,
};
pub use support_patch_b::{
    SMW_US_V1_SUPPORT_PATCH_B_HOOK_OFFSETS, SMW_US_V1_SUPPORT_PATCH_B_RUNTIME_OFFSET,
    SmwUsV1SupportPatchBDetectError, SmwUsV1SupportPatchBInstallError, SmwUsV1SupportPatchBState,
    detect_smw_us_v1_support_patch_b, smw_us_v1_support_patch_b_installation_plan,
    smw_us_v1_support_patch_b_scroll_registers,
};
pub use text::RevisionProfileError;
pub use title_recording::{
    SMW_US_V1_TITLE_RECORDING_HOOK_OFFSET, SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
    SMW_US_V1_TITLE_RECORDING_SEARCH_START, smw_us_v1_title_recording_allocation_policy,
    smw_us_v1_title_recording_locator,
};
pub use title_recording_recorder::{
    SMW_US_V1_TITLE_RECORDER_COMPENSATION_LEN, SMW_US_V1_TITLE_RECORDER_COMPENSATION_OFFSET,
    SMW_US_V1_TITLE_RECORDER_FIRST_HOOK_OFFSET, SMW_US_V1_TITLE_RECORDER_SEARCH_START,
    SMW_US_V1_TITLE_RECORDER_SECOND_HOOK_OFFSET,
    smw_us_v1_title_recording_recorder_allocation_policy,
    smw_us_v1_title_recording_recorder_locator,
};
pub use title_tilemap::{
    SMW_US_V1_TITLE_TILEMAP_POINTER_OFFSET, SMW_US_V1_TITLE_TILEMAP_PRISTINE_STREAM_OFFSET,
    SMW_US_V1_TITLE_TILEMAP_SEARCH_START, smw_us_v1_title_tilemap_allocation_policy,
    smw_us_v1_title_tilemap_locator,
};
pub use vanilla_layer3::{
    SMW_US_V1_LAYER3_BEHAVIOR_TABLE_OFFSET, SMW_US_V1_LAYER3_IMAGE_COUNT,
    SMW_US_V1_LAYER3_IMAGE_POINTER_TABLE_OFFSET, SMW_US_V1_LAYER3_TILEMAP_SIDE,
    SMW_US_V1_LAYER3_TILEMAP_WORDS, SmwUsV1Layer3Behavior, SmwUsV1Layer3Error, SmwUsV1LevelLayer3,
    load_smw_us_v1_level_layer3,
};
pub use vanilla_level_map16::{
    LoadedSmwUsV1LevelMap16Base, SMW_US_V1_MAP16_BACKGROUND_BYTES,
    SMW_US_V1_MAP16_BACKGROUND_OFFSET, SMW_US_V1_MAP16_BASE_BYTES, SMW_US_V1_MAP16_BASE_TILE_COUNT,
    SMW_US_V1_MAP16_COMMON_WORD_OFFSET, SMW_US_V1_MAP16_OCCUPANCY_MASK_BYTES,
    SMW_US_V1_MAP16_OCCUPANCY_MASK_OFFSET, SMW_US_V1_MAP16_SOURCE_BANK_OFFSET,
    SMW_US_V1_MAP16_TILE_BYTES, SMW_US_V1_MAP16_TILESET_WORD_TABLE_OFFSET,
    SmwUsV1LevelMap16BaseError, load_smw_us_v1_background_map16, load_smw_us_v1_level_map16_base,
};
pub use vanilla_level_mode::{
    VanillaLevelMode, smw_us_v1_level_mode, smw_us_v1_secondary_layer_cache_base_cell,
};
pub use vanilla_level_palette::{
    SmwUsV1LevelPalette, SmwUsV1LevelPaletteError, compose_smw_us_v1_level_palette,
};
pub use vanilla_object_family::{VanillaObjectFamily, smw_us_v1_object_family};
pub use vanilla_smw::{
    SMW_US_V1_DEFAULT_MUSIC_TRACKS_OFFSET, SMW_US_V1_ENTRANCE_LEVEL_MODE_AND_SCREEN_OFFSET,
    SMW_US_V1_ENTRANCE_POSITION_OFFSET, SMW_US_V1_ENTRANCE_SCREEN_AND_METHOD_OFFSET,
    SMW_US_V1_ENTRANCE_VERTICAL_SETTINGS_OFFSET, SMW_US_V1_EXPANDED_LEVEL_MODE_HOOK_OFFSETS,
    SMW_US_V1_EXPANDED_LEVEL_MODE_RUNTIME_BIAS, SMW_US_V1_GFX32_STARTUP_POINTER_LOW_OFFSET,
    SMW_US_V1_GFX33_STARTUP_POINTER_LOW_OFFSET, SMW_US_V1_GRAPHICS_POINTER_BANK_OFFSET,
    SMW_US_V1_GRAPHICS_POINTER_HIGH_OFFSET, SMW_US_V1_GRAPHICS_POINTER_LOW_OFFSET,
    SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET, SMW_US_V1_LEVEL_LAYER2_DESCRIPTOR_TABLE_OFFSET,
    SMW_US_V1_LEVEL_LAYER2_FORMAT_103_MARKER_OFFSET, SMW_US_V1_LEVEL_LAYER2_FORMAT_HOOK_OFFSET,
    SMW_US_V1_LEVEL_LAYER2_POINTER_TABLE_OFFSET, SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_OFFSET,
    SMW_US_V1_LEVEL_SPRITE_POINTER_BANK_TABLE_OFFSET, SMW_US_V1_LEVEL_SPRITE_POINTER_HOOK_OFFSET,
    SMW_US_V1_LEVEL_SPRITE_POINTER_LOW_WORD_OFFSET, SMW_US_V1_LFIX3_ADDITIONAL_FLAGS_OFFSET,
    SMW_US_V1_LFIX3_FLAGS_OFFSET, SMW_US_V1_LFIX3_HIGH_POSITION_OFFSET,
    SMW_US_V1_LFIX3_RUNTIME_FLAGS_OFFSET, SMW_US_V1_OBJECT_TILESET_GRAPHICS_OFFSET,
    SMW_US_V1_OBJECT_TILESET_GRAPHICS_SLOTS, SMW_US_V1_OBJECT_TILESETS,
    SMW_US_V1_ORIGINAL_LOGICAL_LEN, SMW_US_V1_SEPARATE_MIDWAY_HOOK_OFFSET,
    SMW_US_V1_SPECIAL_GRAPHICS_FILES, SMW_US_V1_SPECIAL_GRAPHICS_POINTER_OFFSET,
    SMW_US_V1_SPECIAL_GRAPHICS_STARTUP_POINTER_BANK_OFFSET,
    SMW_US_V1_SPRITE_TILESET_GRAPHICS_OFFSET, SMW_US_V1_SPRITE_TILESET_GRAPHICS_SLOTS,
    SMW_US_V1_SPRITE_TILESETS, SMW_US_V1_VANILLA_GRAPHICS_FILES, SMW_US_V1_VANILLA_LEVEL_SLOTS,
    SmwUsV1Layer2LayoutError, SmwUsV1Layer2RuntimeGeneration, SmwUsV1ObjectTilesetGraphicsError,
    SmwUsV1SpecialGraphicsLayoutError, SmwUsV1SpecialGraphicsLayouts,
    SmwUsV1SpriteTilesetGraphicsError, probe_smw_us_v1_layer2_runtime_generation,
    smw_us_v1_default_music_tracks, smw_us_v1_expanded_level_mode_locator, smw_us_v1_layer2_layout,
    smw_us_v1_level_layer2_layout, smw_us_v1_level_uses_shared_background,
    smw_us_v1_lfix3_level_fields_layout, smw_us_v1_object_tileset_graphics_files,
    smw_us_v1_separate_midway_locator, smw_us_v1_special_graphics_layouts,
    smw_us_v1_special_graphics_layouts_for_mapper, smw_us_v1_sprite_pointer_table,
    smw_us_v1_sprite_tileset_graphics_files, smw_us_v1_vanilla_entrance_layout,
    smw_us_v1_vanilla_graphics_layout, smw_us_v1_vanilla_graphics_layout_for_mapper,
    smw_us_v1_vanilla_layer2_layout, smw_us_v1_vanilla_level_layout,
    smw_us_v1_vanilla_special_graphics_layout,
};
pub use vanilla_standard_object_map::{
    SMW_US_V1_STANDARD_OBJECT_FAMILIES, SMW_US_V1_STANDARD_OBJECTS_PER_FAMILY,
    SMW_US_V1_UNKNOWN_STANDARD_OBJECT_DEFINITION, SmwUsV1StandardObjectDefinitionMap,
    SmwUsV1StandardObjectMapError, load_smw_us_v1_standard_object_definition_map,
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
    /// Logical PC offset of the 16-by-4 FG/BG graphics-assignment table loaded from active ROM
    /// layout-descriptor entry `+0x94`. Older profiles may omit it, but workflows that require
    /// object-tileset graphics must then reject instead of assuming the SMW-US-v1 address.
    pub object_tileset_graphics_offset: Option<usize>,
    pub palette: PaletteRomLayout,
    pub palette_installation: InstalledLayout<PaletteRomLayout>,
    pub exanimation: ExAnimationRomLayout,
    pub exanimation_installation: InstalledLayout<InstalledExAnimationRomLayout>,
    pub exanimation_feature_installation:
        InstalledLayout<lm_project::InstalledExAnimationFeatureRomLayout>,
    pub expanded_settings: Option<ExpandedLevelSettingsLayout>,
    pub overworld: CompleteOverworldRomLayout,
    pub overworld_shape: CompleteOverworldShape,
    pub sprite_lengths: SpriteLengthTable,
    pub exanimation_double_size_modes: [bool; 256],
}

pub const OBJECT_TILESET_GRAPHICS_TILESETS: usize = 16;
pub const OBJECT_TILESET_GRAPHICS_SLOTS: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObjectTilesetGraphicsError {
    LayoutMissing,
    TilesetOutOfRange(usize),
    Rom(RomError),
}

impl fmt::Display for ObjectTilesetGraphicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "object-tileset graphics lookup failed: {self:?}")
    }
}

impl std::error::Error for ObjectTilesetGraphicsError {}

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

    /// Reads the four FG/BG graphics-file assignments selected by an object tileset.
    ///
    /// Lunar Magic loads the complete 64-byte source table through active ROM-layout descriptor
    /// entry `+0x94`; this profile field is consequently identity- and mapper-specific.
    pub fn object_tileset_graphics_files(
        &self,
        rom: &RomImage,
        tileset: usize,
    ) -> Result<[usize; OBJECT_TILESET_GRAPHICS_SLOTS], ObjectTilesetGraphicsError> {
        let base = self
            .object_tileset_graphics_offset
            .ok_or(ObjectTilesetGraphicsError::LayoutMissing)?;
        if tileset >= OBJECT_TILESET_GRAPHICS_TILESETS {
            return Err(ObjectTilesetGraphicsError::TilesetOutOfRange(tileset));
        }
        let offset = base
            .checked_add(tileset * OBJECT_TILESET_GRAPHICS_SLOTS)
            .ok_or_else(|| {
                ObjectTilesetGraphicsError::Rom(RomError::RangeOutOfBounds {
                    offset: base,
                    len: OBJECT_TILESET_GRAPHICS_SLOTS,
                    image_len: rom.logical_len(),
                })
            })?;
        let bytes = rom
            .logical_bytes()
            .get(offset..offset + OBJECT_TILESET_GRAPHICS_SLOTS)
            .ok_or_else(|| {
                ObjectTilesetGraphicsError::Rom(RomError::RangeOutOfBounds {
                    offset,
                    len: OBJECT_TILESET_GRAPHICS_SLOTS,
                    image_len: rom.logical_len(),
                })
            })?;
        Ok(std::array::from_fn(|index| usize::from(bytes[index])))
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
        if let Some(offset) = self.object_tileset_graphics_offset {
            let final_offset = offset
                .checked_add(OBJECT_TILESET_GRAPHICS_TILESETS * OBJECT_TILESET_GRAPHICS_SLOTS - 1)
                .ok_or(RevisionProfileError::AddressOverflow(
                    "graphics.object_tileset_assignments",
                ))?;
            pc_to_snes(self.mapper, final_offset).map_err(|_| {
                RevisionProfileError::UnmappedPointerTable("graphics.object_tileset_assignments")
            })?;
        }
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
        self.validate_exanimation_feature_installation()?;
        Ok(())
    }

    fn validate_exanimation_feature_installation(&self) -> Result<(), RevisionProfileError> {
        let validate_marker = |domain, marker: lm_project::InstallationMarker| {
            pc_to_snes(self.mapper, marker.offset)
                .map(|_| ())
                .map_err(|_| RevisionProfileError::UnmappedPointerTable(domain))
        };
        let validate_features =
            |domain, layout: lm_project::InstalledExAnimationFeatureRomLayout| {
                if layout.table_locator.mapper != self.mapper {
                    return Err(RevisionProfileError::MapperMismatch {
                        domain,
                        actual: layout.table_locator.mapper,
                    });
                }
                let final_byte = layout
                    .table_locator
                    .first_operand_offset
                    .checked_add(2)
                    .ok_or(RevisionProfileError::AddressOverflow(domain))?;
                for offset in [layout.table_locator.first_operand_offset, final_byte] {
                    pc_to_snes(self.mapper, offset)
                        .map_err(|_| RevisionProfileError::UnmappedPointerTable(domain))?;
                }
                Ok(())
            };
        match (
            self.exanimation_installation,
            self.exanimation_feature_installation,
        ) {
            (_, lm_project::InstalledLayout::Absent) => {}
            (
                lm_project::InstalledLayout::Unconditional(exanimation),
                lm_project::InstalledLayout::Unconditional(features),
            ) if exanimation.pointer_locator.is_some_and(|locator| {
                locator.first_operand_offset == features.table_locator.first_operand_offset
            }) =>
            {
                validate_features("exanimation.features", features)?;
            }
            (
                lm_project::InstalledLayout::Alternatives {
                    primary: exanimation_primary,
                    fallback: exanimation_fallback,
                },
                lm_project::InstalledLayout::Alternatives {
                    primary: feature_primary,
                    fallback: feature_fallback,
                },
            ) if exanimation_primary.marker == feature_primary.marker
                && exanimation_primary
                    .layout
                    .pointer_locator
                    .is_some_and(|locator| {
                        locator.first_operand_offset
                            == feature_primary.layout.table_locator.first_operand_offset
                    })
                && match (exanimation_fallback, feature_fallback) {
                    (None, None) => true,
                    (Some(exanimation), Some(features)) => {
                        exanimation.marker == features.marker
                            && exanimation.layout.pointer_locator.is_some_and(|locator| {
                                locator.first_operand_offset
                                    == features.layout.table_locator.first_operand_offset
                            })
                    }
                    _ => false,
                } =>
            {
                validate_marker(
                    "exanimation.features.installation_marker",
                    feature_primary.marker,
                )?;
                validate_features("exanimation.features", feature_primary.layout)?;
                if let Some(fallback) = feature_fallback {
                    validate_marker(
                        "exanimation.features.fallback_installation_marker",
                        fallback.marker,
                    )?;
                    validate_features("exanimation.features.fallback", fallback.layout)?;
                }
            }
            _ => {
                return Err(RevisionProfileError::InstallationLayoutMismatch(
                    "exanimation.features",
                ));
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
            if domain == "graphics" && self.graphics.split_pointer_planes.is_some() {
                continue;
            }
            let pointer_span = table_span(domain, table)?;
            if span.start < pointer_span.end && pointer_span.start < span.end {
                return Err(RevisionProfileError::ExpandedSettingsTableOverlap {
                    pointer_table: domain,
                });
            }
        }
        for (domain, pointer_span) in graphics_pointer_spans(self.graphics)? {
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
            if domain != "level.sprites"
                && !(domain == "graphics" && self.graphics.split_pointer_planes.is_some())
            {
                validate_table(self.mapper, domain, table)?;
            }
        }
        let mut spans = tables
            .iter()
            .filter(|(domain, _)| {
                *domain != "level.sprites"
                    && !(*domain == "graphics" && self.graphics.split_pointer_planes.is_some())
            })
            .map(|(domain, table)| Ok((*domain, table_span(domain, *table)?)))
            .collect::<Result<Vec<_>, RevisionProfileError>>()?;
        spans.extend(graphics_pointer_spans(self.graphics)?);
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
        validate_disjoint_spans(&spans)
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

fn validate_disjoint_spans(
    spans: &[(&'static str, Range<usize>)],
) -> Result<(), RevisionProfileError> {
    for first in 0..spans.len() {
        for second in first + 1..spans.len() {
            if spans[first].1.start < spans[second].1.end
                && spans[second].1.start < spans[first].1.end
            {
                return Err(RevisionProfileError::OverlappingPointerTables {
                    first: spans[first].0,
                    second: spans[second].0,
                });
            }
        }
    }
    Ok(())
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

fn graphics_pointer_spans(
    layout: GraphicsRomLayout,
) -> Result<Vec<(&'static str, Range<usize>)>, RevisionProfileError> {
    let Some(planes) = layout.split_pointer_planes else {
        return Ok(Vec::new());
    };
    if planes.low_offset != layout.pointers.offset
        || planes.entries != layout.pointers.entries
        || planes.stride != layout.pointers.stride
    {
        return Err(RevisionProfileError::IncompleteGraphicsPointerLayout);
    }
    let component = |domain, offset| {
        let table = LevelPointerTable {
            offset,
            entries: planes.entries,
            stride: planes.stride,
        };
        validate_component(layout.mapper, domain, table, 1)?;
        Ok((domain, component_span(domain, table, 1)?))
    };
    Ok(vec![
        component("graphics.low", planes.low_offset)?,
        component("graphics.high", planes.high_offset)?,
        component("graphics.bank", planes.bank_offset)?,
    ])
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
