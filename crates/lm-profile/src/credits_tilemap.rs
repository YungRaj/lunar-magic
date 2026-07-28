//! Pristine SMW US revision-0 credits tilemap layout.

use lm_project::{CreditsTilemapPatchLocator, LegacyCreditsTilemapLayout};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_CREDITS_RECORDS_OFFSET: usize = 0x06_15c7;
pub const SMW_US_V1_CREDITS_OFFSETS_OFFSET: usize = 0x06_1d18;
pub const SMW_US_V1_CREDITS_LEGACY_ROWS: usize = 202;
pub const SMW_US_V1_CREDITS_BLANK_WORD: u16 = 0x38fc;
pub const SMW_US_V1_CREDITS_RUNTIME_OFFSET: usize = 0x06_1eeb;
pub const SMW_US_V1_CREDITS_EXPANDED_OFFSETS_OFFSET: usize = 0x06_1cac;
pub const SMW_US_V1_CREDITS_SEARCH_START: usize = 0x08_0000;

const RUNTIME_TEMPLATE: [u8; 0x60] = [
    0x9b, 0xaa, 0xbf, 0xc7, 0x95, 0x0c, 0x85, 0x00, 0xe2, 0x20, 0x64, 0x02, 0xe6, 0x67, 0xc9, 0xff,
    0xc2, 0x21, 0xf0, 0x36, 0x8b, 0xf4, 0x7f, 0x7f, 0xab, 0xab, 0x29, 0xff, 0x00, 0x65, 0x65, 0xeb,
    0x99, 0x7d, 0x83, 0xc8, 0xc8, 0xa5, 0x01, 0xeb, 0x99, 0x7d, 0x83, 0xc8, 0xc8, 0xe8, 0xe8, 0xbf,
    0xc7, 0x95, 0x0c, 0x99, 0x7d, 0x83, 0xc8, 0xc8, 0xe8, 0xe8, 0xc6, 0x01, 0xc6, 0x01, 0x10, 0xef,
    0xa9, 0xff, 0x00, 0x99, 0x7d, 0x83, 0x8c, 0x7b, 0x83, 0xab, 0xe2, 0x10, 0xa5, 0x65, 0x18, 0x69,
    0x20, 0x00, 0x89, 0xff, 0x03, 0xd0, 0x03, 0x49, 0x00, 0x0c, 0x85, 0x65, 0x60, 0xff, 0xff, 0xff,
];

#[must_use]
pub const fn smw_us_v1_legacy_credits_tilemap_layout() -> LegacyCreditsTilemapLayout {
    LegacyCreditsTilemapLayout {
        mapper: Mapper::LoRom,
        records: SMW_US_V1_CREDITS_RECORDS_OFFSET,
        offsets: SMW_US_V1_CREDITS_OFFSETS_OFFSET,
        row_count: SMW_US_V1_CREDITS_LEGACY_ROWS,
        blank_word: SMW_US_V1_CREDITS_BLANK_WORD,
    }
}

#[must_use]
pub fn smw_us_v1_credits_tilemap_locator() -> CreditsTilemapPatchLocator {
    CreditsTilemapPatchLocator {
        mapper: Mapper::LoRom,
        legacy: smw_us_v1_legacy_credits_tilemap_layout(),
        runtime: SMW_US_V1_CREDITS_RUNTIME_OFFSET,
        expanded_offsets: SMW_US_V1_CREDITS_EXPANDED_OFFSETS_OFFSET,
        runtime_template: RUNTIME_TEMPLATE,
    }
}

#[must_use]
pub fn smw_us_v1_credits_allocation_policy(image_len: usize) -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_CREDITS_SEARCH_START..image_len.saturating_add(0x8000).min(0x40_0000),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_credits_load_edit_reopen_and_undo_exactly() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let layout = smw_us_v1_legacy_credits_tilemap_layout();
        let mut tilemap = project.load_legacy_credits_tilemap(layout).unwrap();
        assert!(
            tilemap.words()[202 * 32..]
                .iter()
                .all(|word| *word == SMW_US_V1_CREDITS_BLANK_WORD)
        );
        let unique_row = (0..202)
            .find(|row| {
                let candidate = &tilemap.words()[row * 32..row * 32 + 32];
                (0..202)
                    .filter(|other| tilemap.words()[other * 32..other * 32 + 32] == *candidate)
                    .count()
                    == 1
                    && candidate
                        .iter()
                        .any(|word| *word != SMW_US_V1_CREDITS_BLANK_WORD)
            })
            .unwrap();
        let column = tilemap.words()[unique_row * 32..unique_row * 32 + 32]
            .iter()
            .position(|word| *word != SMW_US_V1_CREDITS_BLANK_WORD)
            .unwrap();
        tilemap.words_mut()[unique_row * 32 + column] ^= 1;
        project
            .save_legacy_credits_tilemap(&tilemap, layout, SMW_US_V1_CHECKSUM_FIELD, 0xff)
            .unwrap();
        assert_eq!(
            project.load_legacy_credits_tilemap(layout).unwrap(),
            tilemap
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn expanded_install_and_update_reopen_and_two_undos_restore_pristine() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let locator = smw_us_v1_credits_tilemap_locator();
        let mut tilemap = project
            .load_credits_tilemap_detected(&locator)
            .unwrap()
            .tilemap;
        tilemap.words_mut()[255 * 32 + 31] = 0x1234;
        let allocation = smw_us_v1_credits_allocation_policy(project.rom.logical_len());
        project
            .save_credits_tilemap_detected(
                &tilemap,
                &locator,
                &allocation,
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert!(matches!(
            project
                .load_credits_tilemap_detected(&locator)
                .unwrap()
                .storage,
            lm_project::CreditsTilemapStorage::Expanded(_)
        ));
        tilemap.words_mut()[254 * 32 + 30] = 0x5678;
        let allocation = smw_us_v1_credits_allocation_policy(project.rom.logical_len());
        project
            .save_credits_tilemap_detected(
                &tilemap,
                &locator,
                &allocation,
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_credits_tilemap_detected(&locator)
                .unwrap()
                .tilemap,
            tilemap
        );
        project.undo().unwrap();
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn pristine_decode_matches_lunar_magic_transfer_fixture() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/credits-transfer-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let before_project =
            lm_project::Project::open_supported(RomImage::from_bytes(before).unwrap()).unwrap();
        let after_project =
            lm_project::Project::open_supported(RomImage::from_bytes(after).unwrap()).unwrap();
        let locator = smw_us_v1_credits_tilemap_locator();
        let before_loaded = before_project
            .load_credits_tilemap_detected(&locator)
            .unwrap();
        let after_loaded = after_project
            .load_credits_tilemap_detected(&locator)
            .unwrap();
        assert!(matches!(
            before_loaded.storage,
            lm_project::CreditsTilemapStorage::Legacy
        ));
        assert!(matches!(
            after_loaded.storage,
            lm_project::CreditsTilemapStorage::Expanded(_)
        ));
        assert_eq!(before_loaded.tilemap, after_loaded.tilemap);
        assert_eq!(
            before_loaded.tilemap.encode_native_file(),
            fs::read(fixture.join("after.lmcred")).unwrap()
        );
    }
}
