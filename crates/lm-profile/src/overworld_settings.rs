//! SMW US revision-0 expanded overworld settings slots.

use lm_level::ExpandedOverworldSettings;
use lm_overworld::{OverworldLayer3SettingsError, OverworldLayer3SettingsTable};
use lm_project::{
    ExpandedLevelSettingsLayout, ExpandedOverworldSettingsIoError,
    OverworldLayer3SettingsRomLayout, Project,
};
use lm_rats::parse_at;
use lm_rom::Mapper;

use crate::{
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START, SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT, SmwUsV1ExpandedSettingsAllocation,
    SmwUsV1ExpandedSettingsAllocationError, smw_us_v1_default_special_expanded_settings_record,
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
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let project = Project::new(
            RomImage::from_bytes(fs::read(root.join("Super Mario World (USA).sfc")).unwrap())
                .unwrap(),
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
    }
}
