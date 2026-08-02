//! SMW US revision-0 expanded overworld settings slots.

use lm_level::ExpandedLevelSettingsRecord;
use lm_level::ExpandedOverworldSettings;
use lm_overworld::{OverworldLayer3SettingsError, OverworldLayer3SettingsTable};
use lm_project::{
    ExpandedLevelSettingsLayout, ExpandedOverworldSettingsIoError,
    OverworldLayer3SettingsRomLayout, Project,
};
use lm_rats::parse_at;
use lm_rom::Mapper;

use crate::{
    ExpandedSettingsEntryContinuation, ExpandedSettingsRuntimeLayout,
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START, SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT, SmwUsV1ExpandedSettingsAllocation,
    SmwUsV1ExpandedSettingsAllocationError, smw_us_v1_default_special_expanded_settings_record,
    smw_us_v1_expanded_settings_fixed_writes,
};

pub const SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT: usize = 0x200;
pub const SMW_US_V1_EXPANDED_SETTINGS_PAYLOAD_OFFSET: usize =
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START + 8;
pub const SMW_US_V1_EXPANDED_SETTINGS_TABLE_OFFSET: usize =
    SMW_US_V1_EXPANDED_SETTINGS_PAYLOAD_OFFSET + SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1OverworldSettings {
    pub settings: ExpandedOverworldSettings,
    pub installed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1ExpandedLevelSettings {
    pub settings: ExpandedLevelSettingsRecord,
    pub installed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSmwUsV1OverworldLayer3Settings {
    pub settings: OverworldLayer3SettingsTable,
    pub installed: bool,
}

#[derive(Debug)]
pub enum SmwUsV1OverworldSettingsLoadError {
    InvalidOwnedBlock,
    WrongOwnedLength(usize),
    Allocation(SmwUsV1ExpandedSettingsAllocationError),
    Settings(ExpandedOverworldSettingsIoError),
    Layer3Settings(OverworldLayer3SettingsError),
}

impl std::fmt::Display for SmwUsV1OverworldSettingsLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "SMW US overworld-settings detection failed: {self:?}"
        )
    }
}

impl std::error::Error for SmwUsV1OverworldSettingsLoadError {}

impl From<SmwUsV1ExpandedSettingsAllocationError> for SmwUsV1OverworldSettingsLoadError {
    fn from(value: SmwUsV1ExpandedSettingsAllocationError) -> Self {
        Self::Allocation(value)
    }
}

impl From<ExpandedOverworldSettingsIoError> for SmwUsV1OverworldSettingsLoadError {
    fn from(value: ExpandedOverworldSettingsIoError) -> Self {
        Self::Settings(value)
    }
}

impl From<OverworldLayer3SettingsError> for SmwUsV1OverworldSettingsLoadError {
    fn from(value: OverworldLayer3SettingsError) -> Self {
        Self::Layer3Settings(value)
    }
}

#[must_use]
pub const fn smw_us_v1_expanded_settings_layout() -> ExpandedLevelSettingsLayout {
    ExpandedLevelSettingsLayout {
        mapper: Mapper::LoRom,
        table_offset: SMW_US_V1_EXPANDED_SETTINGS_TABLE_OFFSET,
        entries: SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT,
        stride: 0x20,
    }
}

/// Returns the semantic Layer 3 view of expanded-settings slots `$200..$206`.
#[must_use]
pub const fn smw_us_v1_overworld_layer3_settings_layout() -> OverworldLayer3SettingsRomLayout {
    let expanded = smw_us_v1_expanded_settings_layout();
    OverworldLayer3SettingsRomLayout {
        mapper: expanded.mapper,
        table_offset: expanded.table_offset
            + SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT * expanded.stride,
    }
}

/// Loads the seven installed special records or materializes Lunar Magic's pristine defaults.
///
/// Absence is recognized only when the recovered allocation header is not a `STAR` tag. A present
/// tag must have the exact allocation length and decode as the recovered expanded-settings
/// allocation before any record is exposed.
///
/// # Errors
///
/// Rejects malformed ownership, allocation framing, record layout, or ROM bounds.
pub fn load_smw_us_v1_overworld_settings(
    project: &Project,
) -> Result<LoadedSmwUsV1OverworldSettings, SmwUsV1OverworldSettingsLoadError> {
    let bytes = project.rom.logical_bytes();
    let header = SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START;
    if bytes.get(header..header + 4) != Some(b"STAR") {
        return Ok(LoadedSmwUsV1OverworldSettings {
            settings: ExpandedOverworldSettings {
                records: std::array::from_fn(|_| {
                    smw_us_v1_default_special_expanded_settings_record()
                }),
            },
            installed: false,
        });
    }
    let block = parse_at(bytes, header)
        .map_err(|_| SmwUsV1OverworldSettingsLoadError::InvalidOwnedBlock)?;
    if block.payload.len() != SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN {
        // `$087FF8` is Lunar Magic's first-fit search start, not an ownership marker. Other
        // Lunar Magic subsystems can legitimately place a different RATS block there before the
        // expanded-settings family is installed (the retained ordinary level-save oracle does
        // exactly that). Only treat the wrong-sized block as damaged expanded-settings storage
        // when the family's fixed runtime destinations are no longer pristine.
        if expanded_settings_runtime_destinations_are_pristine(project) {
            return Ok(default_overworld_settings());
        }
        return Err(SmwUsV1OverworldSettingsLoadError::WrongOwnedLength(
            block.payload.len(),
        ));
    }
    SmwUsV1ExpandedSettingsAllocation::decode(&bytes[block.payload])?;
    Ok(LoadedSmwUsV1OverworldSettings {
        settings: project.load_expanded_overworld_settings(
            SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
            smw_us_v1_expanded_settings_layout(),
        )?,
        installed: true,
    })
}

/// Loads one standard level's expanded settings through the validated owner, or returns the
/// recovered pristine default when the feature is absent.
///
/// # Errors
///
/// Rejects an out-of-range level or malformed installed allocation.
pub fn load_smw_us_v1_expanded_level_settings(
    project: &Project,
    level: usize,
) -> Result<LoadedSmwUsV1ExpandedLevelSettings, SmwUsV1OverworldSettingsLoadError> {
    let bytes = project.rom.logical_bytes();
    let header = SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START;
    if bytes.get(header..header + 4) != Some(b"STAR") {
        return Ok(LoadedSmwUsV1ExpandedLevelSettings {
            settings: crate::smw_us_v1_default_expanded_settings_record(),
            installed: false,
        });
    }
    let block = parse_at(bytes, header)
        .map_err(|_| SmwUsV1OverworldSettingsLoadError::InvalidOwnedBlock)?;
    if block.payload.len() != SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN {
        if expanded_settings_runtime_destinations_are_pristine(project) {
            return Ok(LoadedSmwUsV1ExpandedLevelSettings {
                settings: crate::smw_us_v1_default_expanded_settings_record(),
                installed: false,
            });
        }
        return Err(SmwUsV1OverworldSettingsLoadError::WrongOwnedLength(
            block.payload.len(),
        ));
    }
    let allocation = SmwUsV1ExpandedSettingsAllocation::decode(&bytes[block.payload])?;
    Ok(LoadedSmwUsV1ExpandedLevelSettings {
        settings: allocation.record(level)?.clone(),
        installed: true,
    })
}

fn default_overworld_settings() -> LoadedSmwUsV1OverworldSettings {
    LoadedSmwUsV1OverworldSettings {
        settings: ExpandedOverworldSettings {
            records: std::array::from_fn(|_| smw_us_v1_default_special_expanded_settings_record()),
        },
        installed: false,
    }
}

fn expanded_settings_runtime_destinations_are_pristine(project: &Project) -> bool {
    let layout = ExpandedSettingsRuntimeLayout::smw_us_v1(
        0x11_8000,
        ExpandedSettingsEntryContinuation::Continue,
    );
    smw_us_v1_expanded_settings_fixed_writes(layout).is_ok_and(|writes| {
        writes.iter().all(|write| {
            project
                .rom
                .read(write.offset, write.expected.len())
                .is_ok_and(|bytes| bytes == write.expected)
        })
    })
}

/// Loads the seven records through the validated expanded-settings owner and gives them their
/// recovered overworld Layer 3 semantics.
///
/// # Errors
///
/// Rejects malformed ownership, allocation framing, record layout, or ROM bounds.
pub fn load_smw_us_v1_overworld_layer3_settings(
    project: &Project,
) -> Result<LoadedSmwUsV1OverworldLayer3Settings, SmwUsV1OverworldSettingsLoadError> {
    let loaded = load_smw_us_v1_overworld_settings(project)?;
    let mut bytes = [0; OverworldLayer3SettingsTable::ENCODED_LEN];
    for (index, record) in loaded.settings.records.iter().enumerate() {
        let start = index * record.encoded().len();
        bytes[start..start + record.encoded().len()].copy_from_slice(record.encoded());
    }
    Ok(LoadedSmwUsV1OverworldLayer3Settings {
        settings: OverworldLayer3SettingsTable::decode(&bytes)?,
        installed: loaded.installed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_project::Project;
    use lm_rom::{RomImage, SnesChecksum};
    use std::{fs, path::PathBuf};

    #[test]
    fn retained_wine_rom_loads_and_updates_all_seven_exact_records_atomically() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
        let layout = smw_us_v1_expanded_settings_layout();
        let mut settings = project
            .load_expanded_overworld_settings(SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT, layout)
            .unwrap();
        assert_eq!(settings.records.len(), 7);
        assert_eq!(settings.records[0].word(0).unwrap(), 0x14);
        settings.records[6].set_word(11, 0x345).unwrap();
        assert!(
            project
                .save_expanded_overworld_settings(
                    SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT,
                    &settings,
                    layout,
                    SMW_US_V1_CHECKSUM_FIELD,
                )
                .unwrap()
        );
        assert_eq!(
            project
                .load_expanded_overworld_settings(SMW_US_V1_OVERWORLD_SETTINGS_FIRST_SLOT, layout)
                .unwrap(),
            settings
        );
        assert!(
            SnesChecksum::decode(project.rom.logical_bytes(), SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
                .is_complementary()
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), bytes);
    }

    #[test]
    fn pristine_detection_materializes_defaults_without_claiming_installation() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(crate::test_support::pristine_smw_us_rom_bytes()).unwrap(),
        );
        let loaded = load_smw_us_v1_overworld_settings(&project).unwrap();
        assert!(!loaded.installed);
        assert!(
            loaded
                .settings
                .records
                .iter()
                .all(|record| { record == &smw_us_v1_default_special_expanded_settings_record() })
        );
        let level = load_smw_us_v1_expanded_level_settings(&project, 0).unwrap();
        assert!(!level.installed);
        assert_eq!(
            level.settings,
            crate::smw_us_v1_default_expanded_settings_record()
        );
    }

    #[test]
    fn unrelated_first_fit_rats_block_does_not_claim_expanded_settings_ownership() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let block = parse_at(
            project.rom.logical_bytes(),
            SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START,
        )
        .unwrap();
        assert_eq!(block.payload.len(), 0x8000);

        let overworld = load_smw_us_v1_overworld_settings(&project).unwrap();
        assert!(!overworld.installed);
        let level = load_smw_us_v1_expanded_level_settings(&project, 0x102).unwrap();
        assert!(!level.installed);
        assert_eq!(
            level.settings,
            crate::smw_us_v1_default_expanded_settings_record()
        );
    }

    #[test]
    fn wrong_length_owner_still_rejects_when_expanded_runtime_is_installed() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let image = RomImage::from_bytes(bytes).unwrap();
        let mut logical = image.logical_bytes().to_vec();
        let header = SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START;
        logical[header + 4..header + 6].copy_from_slice(&0x7fff_u16.to_le_bytes());
        logical[header + 6..header + 8].copy_from_slice(&0x8000_u16.to_le_bytes());
        let project = Project::new(RomImage::from_bytes(logical).unwrap());

        assert!(matches!(
            load_smw_us_v1_overworld_settings(&project),
            Err(SmwUsV1OverworldSettingsLoadError::WrongOwnedLength(0x8000))
        ));
        assert!(matches!(
            load_smw_us_v1_expanded_level_settings(&project, 0),
            Err(SmwUsV1OverworldSettingsLoadError::WrongOwnedLength(0x8000))
        ));
    }

    #[test]
    fn retained_wine_rom_exposes_the_same_slots_with_layer3_semantics() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = fs::read(
            root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/after.smc"),
        )
        .unwrap();
        let project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let loaded = load_smw_us_v1_overworld_layer3_settings(&project).unwrap();
        assert!(loaded.installed);
        assert_eq!(loaded.settings.maps[0].feature_flags(), 0x14);
        assert_eq!(
            loaded.settings,
            project
                .load_overworld_layer3_settings(smw_us_v1_overworld_layer3_settings_layout())
                .unwrap()
        );
        let level = load_smw_us_v1_expanded_level_settings(&project, 0).unwrap();
        assert!(level.installed);
        assert_eq!(
            level.settings,
            project
                .load_expanded_level_settings(0, smw_us_v1_expanded_settings_layout())
                .unwrap()
        );
    }
}
