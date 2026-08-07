//! SMW-US revision-0 layout for Lunar Magic's permanent level-access restriction.

use lm_project::LevelAccessRestrictionLayout;
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
        title: 0x0000_7fc0,
        version: 0x0000_7fdb,
        checksum_field: crate::SMW_US_V1_CHECKSUM_FIELD,
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
