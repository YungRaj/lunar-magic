//! SMW-US revision-0 layout for Lunar Magic's permanent level-access restriction.

use lm_project::{ExLoRomRestrictionBulkSaveLayout, LevelAccessRestrictionLayout};
use lm_rom::Mapper;

/// Returns the headerless locations recovered from Lunar Magic 3.63's active `LoROM` descriptor.
#[must_use]
pub const fn smw_us_v1_level_access_restriction_layout() -> LevelAccessRestrictionLayout {
    LevelAccessRestrictionLayout {
        mapper: Mapper::LoRom,
        per_save_hook: 0x0002_8605,
        per_save_code: 0x0006_f100,
        per_save_completion_marker: 0x0001_bb1f,
        bulk_save_hook: 0x0000_38de,
        bulk_save_code: 0x0006_f1a0,
        graphics_pointer_low: 0x0000_3992,
        graphics_pointer_high: 0x0000_39c4,
        graphics_pointer_entries: 0x32,
        graphics_integrity_words: [0x0000_388b, 0x0000_38d8],
        protected_pointer_words: [
            0x0008_000d,
            0x0008_0010,
            0x0008_0013,
            0x0008_0016,
            0x0008_0019,
            0x0008_001c,
            0x0008_001f,
            0x0008_0022,
            0x0008_0025,
        ],
        metadata_compensation_fill: 0x0007_f005,
        metadata_compensation_len: 0x19,
        metadata_compensation_byte: 0x0007_f01e,
        restriction_marker: 0x0000_7fbf,
        restriction_marker_mirror: None,
        title: 0x0000_7fc0,
        title_mirror: None,
        version: 0x0000_7fdb,
        version_mirror: None,
        checksum_field: crate::SMW_US_V1_CHECKSUM_FIELD,
        exlorom_bulk_save: None,
    }
}

/// Returns the exact descriptor-routed layout used after the 64-Mbit ExLoROM conversion.
#[must_use]
pub const fn smw_us_v1_exlorom_level_access_restriction_layout() -> LevelAccessRestrictionLayout {
    LevelAccessRestrictionLayout {
        mapper: Mapper::ExLoRom,
        per_save_hook: 0x0042_8605,
        per_save_code: 0x0046_f100,
        per_save_completion_marker: 0x0041_bb1f,
        bulk_save_hook: 0x0040_38de,
        bulk_save_code: 0x0046_f1a0,
        graphics_pointer_low: 0x0040_3992,
        graphics_pointer_high: 0x0040_39c4,
        graphics_pointer_entries: 0x32,
        graphics_integrity_words: [0x0040_388b, 0x0040_38d8],
        protected_pointer_words: [0; 9],
        metadata_compensation_fill: 0x0047_f015,
        metadata_compensation_len: 0x19,
        metadata_compensation_byte: 0x0047_f02e,
        restriction_marker: 0x0000_7fbf,
        restriction_marker_mirror: Some(0x0040_7fbf),
        title: 0x0000_7fc0,
        title_mirror: Some(0x0040_7fc0),
        version: 0x0000_7fdb,
        version_mirror: Some(0x0040_7fdb),
        checksum_field: 0x0040_7fdc,
        exlorom_bulk_save: Some(ExLoRomRestrictionBulkSaveLayout {
            protected_owner: 0x0048_0000,
            auxiliary_owner: 0x0048_0541,
            allocation_start: 0x0020_0000,
            allocation_end: 0x0040_0000,
            protected_pointer: 0x0042_e000,
            auxiliary_pointer_low: 0x0042_ec00,
            auxiliary_pointer_bank: 0x0047_7100,
            allocation_cursor: 0x0047_fffc,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{LevelAccessRestrictionError, LevelAccessRestrictionKeys, Project};
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    fn authenticated_project() -> (Project, Vec<u8>) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let project =
            Project::open_supported(RomImage::from_bytes(source.clone()).unwrap()).unwrap();
        (project, source)
    }

    const KEYS: LevelAccessRestrictionKeys = LevelAccessRestrictionKeys {
        per_save_low: 0x35,
        per_save_high: 0xc7,
        graphics: 0x7c35,
    };

    #[test]
    fn restriction_matches_complete_authenticated_lunar_magic_rom() {
        let (mut project, source) = authenticated_project();

        project
            .restrict_level_access(
                "Codex Parity Test",
                KEYS,
                smw_us_v1_level_access_restriction_layout(),
            )
            .unwrap();

        let output = project.save_snapshot();
        assert_eq!(fnv1a64(&output), 0x3359_4e98_bc23_6465);
        assert_eq!(&output[0x81bf..0x81d5], b"BCodex Parity Test    ");
        assert_eq!(
            lm_rom::compute_snes_checksum(
                project.rom.logical_bytes(),
                crate::SMW_US_V1_CHECKSUM_FIELD
            )
            .unwrap(),
            lm_rom::SnesChecksum::decode(
                project.rom.logical_bytes(),
                crate::SMW_US_V1_CHECKSUM_FIELD
            )
            .unwrap()
        );

        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), source);
        project.redo().unwrap();
        assert_eq!(project.save_snapshot(), output);
    }

    #[test]
    fn invalid_titles_are_rejected_without_mutating_the_project() {
        for title in ["This title is much too long", "Non-ASCII \u{00e9}"] {
            let (mut project, source) = authenticated_project();
            let error = project
                .restrict_level_access(title, KEYS, smw_us_v1_level_access_restriction_layout())
                .unwrap_err();
            assert!(matches!(
                error,
                LevelAccessRestrictionError::TitleTooLong(_)
                    | LevelAccessRestrictionError::NonAsciiTitle
            ));
            assert_eq!(project.save_snapshot(), source);
            assert!(!project.history.can_undo());
        }
    }

    #[test]
    fn a_second_restriction_is_rejected_without_partial_mutation() {
        let (mut project, _) = authenticated_project();
        project
            .restrict_level_access(
                "Codex Parity Test",
                KEYS,
                smw_us_v1_level_access_restriction_layout(),
            )
            .unwrap();
        let restricted = project.save_snapshot();

        let error = project
            .restrict_level_access(
                "Different title",
                LevelAccessRestrictionKeys {
                    per_save_low: 1,
                    per_save_high: 2,
                    graphics: 0x0403,
                },
                smw_us_v1_level_access_restriction_layout(),
            )
            .unwrap_err();

        assert!(matches!(
            error,
            LevelAccessRestrictionError::AlreadyRestricted
        ));
        assert_eq!(project.save_snapshot(), restricted);
        project.undo().unwrap();
        assert!(!project.history.can_undo());
    }

    #[test]
    fn exlorom_variant_is_rejected_atomically_instead_of_using_lorom_offsets() {
        let (mut project, _) = authenticated_project();
        project.convert_to_64_mbit_exlorom().unwrap();
        let converted = project.save_snapshot();
        let history_before = project.history.undo_len();

        let error = project
            .restrict_level_access(
                "Wrong mapper must fail",
                KEYS,
                smw_us_v1_level_access_restriction_layout(),
            )
            .unwrap_err();

        assert!(
            matches!(
                error,
                LevelAccessRestrictionError::MapperMismatch {
                    expected: Mapper::ExLoRom,
                    actual: Mapper::LoRom,
                }
            ),
            "unexpected restriction error: {error:?}"
        );
        assert_eq!(project.save_snapshot(), converted);
        assert_eq!(project.history.undo_len(), history_before);
    }

    #[test]
    fn exlorom_restriction_matches_authenticated_bulk_save_and_undoes_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(root.join("oracle-work/lm363/restrict-exlorom/before.smc")).unwrap();
        let expected = fs::read(root.join("oracle-work/lm363/restrict-exlorom/after.smc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(before.clone()).unwrap()).unwrap();

        project
            .restrict_level_access(
                "Codex Parity Test",
                LevelAccessRestrictionKeys {
                    per_save_low: 0x32,
                    per_save_high: 0x5d,
                    graphics: 0x0b32,
                },
                smw_us_v1_exlorom_level_access_restriction_layout(),
            )
            .unwrap();

        assert_eq!(project.save_snapshot(), expected);
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), before);
        project.redo().unwrap();
        assert_eq!(project.save_snapshot(), expected);
    }

    #[test]
    fn exlorom_restriction_is_logically_identical_without_a_copier_header() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(root.join("oracle-work/lm363/restrict-exlorom/before.smc")).unwrap();
        let expected = fs::read(root.join("oracle-work/lm363/restrict-exlorom/after.smc")).unwrap();
        let logical_before = before[lm_rom::COPIER_HEADER_LEN..].to_vec();
        let logical_expected = expected[lm_rom::COPIER_HEADER_LEN..].to_vec();
        let mut project =
            Project::open_supported(RomImage::from_bytes(logical_before.clone()).unwrap()).unwrap();

        project
            .restrict_level_access(
                "Codex Parity Test",
                LevelAccessRestrictionKeys {
                    per_save_low: 0x32,
                    per_save_high: 0x5d,
                    graphics: 0x0b32,
                },
                smw_us_v1_exlorom_level_access_restriction_layout(),
            )
            .unwrap();

        assert_eq!(project.save_snapshot(), logical_expected);
        assert!(project.rom.copier_header_bytes().is_none());
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), logical_before);
        assert!(project.rom.copier_header_bytes().is_none());
    }

    #[test]
    fn exlorom_bulk_save_relocates_around_occupied_space_and_undoes_atomically() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = fs::read(root.join("oracle-work/lm363/restrict-exlorom/before.smc")).unwrap();
        let mut project = Project::open_supported(RomImage::from_bytes(source).unwrap()).unwrap();
        project.rom.write(0x20_0000, &[0x7f]).unwrap();
        let checksum =
            lm_rom::compute_snes_checksum(project.rom.logical_bytes(), 0x40_7fdc).unwrap();
        project.rom.write(0x40_7fdc, &checksum.encoded()).unwrap();
        let before = project.save_snapshot();

        project
            .restrict_level_access(
                "Relocated protection",
                LevelAccessRestrictionKeys {
                    per_save_low: 0x32,
                    per_save_high: 0x5d,
                    graphics: 0x0b32,
                },
                smw_us_v1_exlorom_level_access_restriction_layout(),
            )
            .unwrap();

        assert_eq!(project.rom.read(0x20_0000, 5).unwrap(), b"\x7fSTAR");
        assert_eq!(
            lm_rom::compute_snes_checksum(project.rom.logical_bytes(), 0x40_7fdc).unwrap(),
            lm_rom::SnesChecksum::decode(project.rom.logical_bytes(), 0x40_7fdc).unwrap()
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), before);
    }

    #[test]
    fn corrupt_exlorom_bulk_save_owner_is_rejected_without_partial_mutation() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = fs::read(root.join("oracle-work/lm363/restrict-exlorom/before.smc")).unwrap();
        let mut project = Project::open_supported(RomImage::from_bytes(source).unwrap()).unwrap();
        project.rom.write(0x48_0000, b"BROK").unwrap();
        let before = project.save_snapshot();
        let history_before = project.history.undo_len();

        let error = project
            .restrict_level_access(
                "Must remain atomic",
                KEYS,
                smw_us_v1_exlorom_level_access_restriction_layout(),
            )
            .unwrap_err();

        assert!(matches!(error, LevelAccessRestrictionError::InvalidLayout));
        assert_eq!(project.save_snapshot(), before);
        assert_eq!(project.history.undo_len(), history_before);
    }

    const fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325;
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            index += 1;
        }
        hash
    }
}
