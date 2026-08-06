//! Differential-test manifests and byte-range comparison.

mod allocation;
mod capture;
mod corpus;
mod diff;
mod hash;
mod manifest;
mod observation;
mod observe_animation_frame;
mod observe_appearances;
mod observe_boss_sequence;
mod observe_codec;
mod observe_complete_overworld;
mod observe_custom_objects;
mod observe_custom_overworld_sprites;
mod observe_custom_sprites;
mod observe_dsc_sidecar;
mod observe_event_reveals;
mod observe_event_tilemap;
mod observe_exanimation_slot_options;
mod observe_expanded_settings;
mod observe_graphics;
mod observe_graphics_remap;
mod observe_level;
mod observe_live_level_map16;
mod observe_lunar_magic_metadata;
mod observe_map16_remaps;
mod observe_mwl;
mod observe_mwl_optional_assets;
mod observe_native_level;
mod observe_native_level_assets;
mod observe_native_map16_sidecar;
mod observe_native_overworld_appearances;
mod observe_overworld;
mod observe_overworld_layer3_settings;
mod observe_rats;
mod observe_rats_manifest;
mod observe_raw_palette;
mod observe_revision_patch;
mod observe_rgb_palette;
mod observe_scene_tilemaps;
mod observe_secondary_exits;
mod observe_smw_palette;
mod observe_title_recording;
mod observe_tpl_palette;
mod observe_transfer_overworld;
mod observe_transferred_map16;
mod verify;

pub use allocation::{
    OwnedTaggedRangeReport, TaggedAllocationDiff, TaggedPayloadIdentity, TaggedPayloadMatch,
    compare_tagged_allocations, verify_owned_tagged_ranges,
};
pub use capture::{AllocationOwnershipPolicy, CaptureMetadata, capture_oracle_case};
pub use corpus::{
    CorpusCoverageReport, CorpusPolicy, CorpusRequirement, InvalidCorpusCase, audit_corpus,
};
pub use diff::{ByteDifference, compare_bytes, unexpected_differences};
pub use hash::{sha256, sha256_hex};
pub use manifest::{ManifestError, Operation, OracleManifest};
pub use observation::{
    Observation, ObservationDifference, ObservationError, SemanticVerificationReport,
    verify_semantic_observations,
};
pub use observe_animation_frame::observe_materialized_animation_frame;
pub use observe_appearances::{observe_entity_appearances, observe_overworld_appearances};
pub use observe_boss_sequence::observe_boss_sequence_messages;
pub use observe_codec::{CodecObservationError, CodecObservationKind, observe_codec};
pub use observe_complete_overworld::observe_complete_overworld;
pub use observe_custom_objects::observe_custom_object_library;
pub use observe_custom_overworld_sprites::observe_custom_overworld_sprites;
pub use observe_custom_sprites::observe_custom_sprite_library;
pub use observe_dsc_sidecar::{
    observe_dsc_display, observe_dsc_materialization, observe_dsc_sidecar,
};
pub use observe_event_reveals::observe_event_reveals;
pub use observe_event_tilemap::observe_event_tilemap_buffers;
pub use observe_exanimation_slot_options::observe_exanimation_slot_options;
pub use observe_expanded_settings::observe_expanded_settings;
pub use observe_graphics::{
    ExAnimationObservationError, observe_compact_exanimation,
    observe_compact_exanimation_with_modes, observe_exanimation, observe_graphics, observe_palette,
};
pub use observe_graphics_remap::observe_graphics_remap;
pub use observe_level::{
    observe_layer3, observe_level, observe_map16_page, observe_map16_page_file, observe_map16_set,
};
pub use observe_live_level_map16::{
    LIVE_LEVEL_MAP16_BYTES, LIVE_LEVEL_MAP16_CELLS, LiveLevelMap16ObservationError,
    observe_live_level_map16,
};
pub use observe_lunar_magic_metadata::observe_lunar_magic_rom_metadata;
pub use observe_map16_remaps::observe_map16_remaps;
pub use observe_mwl::observe_mwl;
pub use observe_mwl_optional_assets::observe_mwl_optional_assets;
pub use observe_native_level::observe_native_level;
pub use observe_native_level_assets::observe_native_level_assets;
pub use observe_native_map16_sidecar::{observe_m16_sidecar, observe_s16_sidecar};
pub use observe_native_overworld_appearances::observe_native_overworld_appearances;
pub use observe_overworld::{
    observe_overworld, observe_overworld_messages, observe_overworld_metadata,
    observe_overworld_paths, observe_overworld_sprites,
};
pub use observe_overworld_layer3_settings::observe_overworld_layer3_settings;
pub use observe_rats::observe_rats;
pub use observe_rats_manifest::observe_rats_manifest;
pub use observe_raw_palette::{observe_palette_mask, observe_raw_palette};
pub use observe_revision_patch::observe_revision_patch;
pub use observe_rgb_palette::observe_rgb_palette;
pub use observe_scene_tilemaps::{observe_credits_tilemap, observe_expanded_layer_tilemap};
pub use observe_secondary_exits::{ObserveSecondaryExitError, observe_secondary_exit_table};
pub use observe_smw_palette::observe_smw_palette;
pub use observe_title_recording::observe_title_recording;
pub use observe_tpl_palette::observe_tpl_palette;
pub use observe_transfer_overworld::{
    TransferOverworldDomains, observe_transfer_overworld, observe_transfer_overworld_events,
};
pub use observe_transferred_map16::observe_transferred_map16;
pub use verify::{
    OracleCaseReport, VerificationReport, verify_manifest_change, verify_oracle_case,
    verify_oracle_case_with_observations,
};
