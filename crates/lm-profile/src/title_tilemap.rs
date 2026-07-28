//! Pristine SMW US revision-0 title-screen Layer 3 tilemap metadata.

use lm_project::TitleTilemapPatchLocator;
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_TITLE_TILEMAP_POINTER_OFFSET: usize = 0x0000_04d3;
pub const SMW_US_V1_TITLE_TILEMAP_PRISTINE_STREAM_OFFSET: usize = 0x0002_b375;
pub const SMW_US_V1_TITLE_TILEMAP_SEARCH_START: usize = 0x08_0000;

#[must_use]
pub const fn smw_us_v1_title_tilemap_locator() -> TitleTilemapPatchLocator {
    TitleTilemapPatchLocator {
        mapper: Mapper::LoRom,
        pointer_operand: SMW_US_V1_TITLE_TILEMAP_POINTER_OFFSET,
        pristine_stream: SMW_US_V1_TITLE_TILEMAP_PRISTINE_STREAM_OFFSET,
    }
}

#[must_use]
pub fn smw_us_v1_title_tilemap_allocation_policy(image_len: usize) -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_TITLE_TILEMAP_SEARCH_START..image_len.saturating_add(0x8000).min(0x40_0000),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_project::TitleTilemapStorage;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_install_update_reopen_and_two_undos_restore_exact_rom() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let locator = smw_us_v1_title_tilemap_locator();
        let loaded = project.load_title_tilemap_detected(locator).unwrap();
        assert_eq!(loaded.storage, TitleTilemapStorage::Pristine);
        let mut tilemap = loaded.tilemap;
        tilemap.primary_bytes_mut()[0] ^= 1;
        project
            .save_title_tilemap_detected(
                &tilemap,
                locator,
                &smw_us_v1_title_tilemap_allocation_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert!(matches!(
            project
                .load_title_tilemap_detected(locator)
                .unwrap()
                .storage,
            TitleTilemapStorage::Expanded(_)
        ));
        tilemap.secondary_bytes_mut()[0] = 0x55;
        project
            .save_title_tilemap_detected(
                &tilemap,
                locator,
                &smw_us_v1_title_tilemap_allocation_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_title_tilemap_detected(locator)
                .unwrap()
                .tilemap,
            tilemap
        );
        project.undo().unwrap();
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn pristine_decode_matches_lunar_magic_transfer_fixture_and_rust_reinstall() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/title-screen-transfer-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let locator = smw_us_v1_title_tilemap_locator();
        let mut pristine =
            lm_project::Project::open_supported(RomImage::from_bytes(before).unwrap()).unwrap();
        let transferred =
            lm_project::Project::open_supported(RomImage::from_bytes(after).unwrap()).unwrap();
        let expected = transferred
            .load_title_tilemap_detected(locator)
            .unwrap()
            .tilemap;
        let decoded = pristine
            .load_title_tilemap_detected(locator)
            .unwrap()
            .tilemap;
        assert_eq!(decoded, expected);
        assert_eq!(
            decoded.encode_native_file(),
            fs::read(fixture.join("after.lmtile")).unwrap()
        );

        pristine
            .save_title_tilemap_detected(
                &decoded,
                locator,
                &smw_us_v1_title_tilemap_allocation_policy(pristine.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            pristine
                .load_title_tilemap_detected(locator)
                .unwrap()
                .tilemap,
            expected
        );
    }
}
