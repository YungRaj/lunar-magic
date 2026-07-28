//! SMW US revision-0 fixed Lunar Magic attribution and feature-record locations.

use lm_project::LunarMagicRomMetadataLayout;
use lm_rom::Mapper;

pub const SMW_US_V1_LM_ATTRIBUTION_OFFSET: usize = 0x0007_f0a0;
pub const SMW_US_V1_LM_VRAM_VERSION_OFFSET: usize = 0x0007_ffe6;
pub const SMW_US_V1_LM_FEATURE_RECORD_OFFSET: usize = 0x0007_ffe7;

#[must_use]
pub const fn smw_us_v1_lunar_magic_metadata_layout() -> LunarMagicRomMetadataLayout {
    LunarMagicRomMetadataLayout {
        mapper: Mapper::LoRom,
        attribution: SMW_US_V1_LM_ATTRIBUTION_OFFSET,
        vram_version: SMW_US_V1_LM_VRAM_VERSION_OFFSET,
        feature_record: SMW_US_V1_LM_FEATURE_RECORD_OFFSET,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_rom::{LunarMagicRomMetadata, RomImage};
    use std::{fs, path::PathBuf};

    #[test]
    fn real_lm363_level_save_metadata_decodes_and_round_trips_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let project =
            lm_project::Project::open_supported(RomImage::from_bytes(fixture).unwrap()).unwrap();
        let metadata = project
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .unwrap()
            .unwrap();
        assert_eq!(metadata.vram_version(), 1);
        assert_eq!(metadata.feature_bits(), 0xfff8_0000);
        assert_eq!(metadata.runtime_pointer(0), Some(0xff_1000));
        assert_eq!(metadata.runtime_pointer(4), Some(0x08_074e));
        assert_eq!(
            LunarMagicRomMetadata::decode_file(&metadata.encode_file()).unwrap(),
            metadata
        );
    }

    #[test]
    fn metadata_install_reopen_and_undo_restore_pristine_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let fixture =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let fixture_project =
            lm_project::Project::open_supported(RomImage::from_bytes(fixture).unwrap()).unwrap();
        let layout = smw_us_v1_lunar_magic_metadata_layout();
        let metadata = fixture_project
            .load_lunar_magic_rom_metadata(layout)
            .unwrap()
            .unwrap();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        assert_eq!(project.load_lunar_magic_rom_metadata(layout).unwrap(), None);
        project
            .save_lunar_magic_rom_metadata(&metadata, layout, SMW_US_V1_CHECKSUM_FIELD)
            .unwrap();
        assert_eq!(
            project.load_lunar_magic_rom_metadata(layout).unwrap(),
            Some(metadata)
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }
}
