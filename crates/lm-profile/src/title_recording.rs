//! SMW US revision-0 title-screen playback runtime metadata.

use lm_project::TitleRecordingPatchLocator;
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_TITLE_RECORDING_HOOK_OFFSET: usize = 0x0000_1c6f;
pub const SMW_US_V1_TITLE_RECORDING_SEARCH_START: usize = 0x0006_abf7;

const PRISTINE_HOOK: [u8; 0x11] = [
    0xae, 0xf4, 0x1d, 0xce, 0xf5, 0x1d, 0xd0, 0x0b, 0xbd, 0x20, 0x9c, 0x8d, 0xf5, 0x1d, 0xe8, 0xe8,
    0x8e,
];

const HOOK_TEMPLATE: [u8; 0x11] = [
    0x22, 0x00, 0x80, 0x00, 0xc9, 0xff, 0xf0, 0x03, 0x4c, 0xda, 0xa1, 0xa0, 0x02, 0x8c, 0x00, 0x01,
    0x60,
];

const RUNTIME_TEMPLATE: [u8; 0x60] = [
    0x08, 0xc2, 0x20, 0xae, 0xf4, 0x1d, 0xd0, 0x0c, 0xa9, 0x00, 0x00, 0x8f, 0xfe, 0xff, 0x7f, 0xa2,
    0x01, 0x8e, 0xf4, 0x1d, 0xc2, 0x10, 0xaf, 0xfe, 0xff, 0x7f, 0xaa, 0xe2, 0x20, 0xce, 0xf5, 0x1d,
    0xd0, 0x13, 0xbf, 0x00, 0x80, 0x00, 0x8d, 0xf5, 0x1d, 0xe8, 0xe8, 0xe8, 0xc2, 0x20, 0x8a, 0x8f,
    0xfe, 0xff, 0x7f, 0xe2, 0x20, 0xbf, 0x00, 0x80, 0x00, 0xc9, 0xff, 0xf0, 0x20, 0x85, 0x15, 0x29,
    0x3f, 0x85, 0x16, 0xbf, 0x00, 0x80, 0x00, 0xa8, 0x0a, 0x0a, 0x0a, 0x0a, 0x29, 0xc0, 0x05, 0x16,
    0x85, 0x16, 0x98, 0x29, 0xb0, 0x85, 0x17, 0x98, 0x0a, 0x29, 0x80, 0x85, 0x18, 0x28, 0x6b, 0xff,
];

#[must_use]
pub const fn smw_us_v1_title_recording_locator() -> TitleRecordingPatchLocator {
    TitleRecordingPatchLocator {
        mapper: Mapper::LoRom,
        hook: SMW_US_V1_TITLE_RECORDING_HOOK_OFFSET,
        pristine_hook: PRISTINE_HOOK,
        hook_template: HOOK_TEMPLATE,
        runtime_template: RUNTIME_TEMPLATE,
        checksum_compensation: Some(0x0007_efa3..0x0007_f08e),
    }
}

#[must_use]
pub fn smw_us_v1_title_recording_allocation_policy(image_len: usize) -> AllocationPolicy {
    let mut policy = AllocationPolicy::lorom(
        SMW_US_V1_TITLE_RECORDING_SEARCH_START.max(0x08_0000)
            ..image_len.saturating_add(0x8000).min(0x40_0000),
    );
    // Lunar Magic selects zero-filled expanded-ROM space for this subsystem. Bytes below the
    // original 512 KiB boundary remain authored vanilla tables even when they contain long zero
    // runs; treating those runs as free corrupts level data. Ordinary $FF runs are not selected.
    policy.fill_bytes = vec![0x00];
    policy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_project::TitleRecordingStorage;
    use lm_rom::RomImage;
    use lm_title::{TitleScreenRecording, encode_zsnes_title_recording};
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    fn recording(value: u8, length: usize) -> TitleScreenRecording {
        let mut bytes = vec![value; length];
        *bytes.last_mut().unwrap() = 0xff;
        TitleScreenRecording::from_bytes(bytes).unwrap()
    }

    #[test]
    fn pristine_install_update_reopen_and_two_undos_restore_exact_rom() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let locator = smw_us_v1_title_recording_locator();
        assert_eq!(
            project
                .load_title_recording_detected(&locator)
                .unwrap()
                .storage,
            TitleRecordingStorage::Absent
        );
        let first = recording(0x12, 7);
        project
            .save_title_recording_detected(
                &first,
                &locator,
                &smw_us_v1_title_recording_allocation_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_title_recording_detected(&locator)
                .unwrap()
                .recording,
            Some(first)
        );
        let second = recording(0x56, 0x101);
        project
            .save_title_recording_detected(
                &second,
                &locator,
                &smw_us_v1_title_recording_allocation_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_title_recording_detected(&locator)
                .unwrap()
                .recording,
            Some(second)
        );
        project.undo().unwrap();
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn playback_import_matches_retained_lunar_magic_oracle_byte_for_byte() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(original) = std::fs::read(root.join("Super Mario World (USA).sfc")) else {
            return;
        };
        if format!("{:x}", Sha256::digest(&original))
            != "7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7"
        {
            return;
        }
        let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let locator = smw_us_v1_title_recording_locator();
        project
            .save_title_recording_detected(
                &recording,
                &locator,
                &smw_us_v1_title_recording_allocation_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        let installed = project.save_snapshot();
        assert_eq!(installed.len(), original.len());
        assert_eq!(
            format!("{:x}", Sha256::digest(&installed)),
            "758c41d8f849d2a96efa76f789f471b37e2843981f0b759e34c6a670cc936676"
        );
        assert_eq!(
            original
                .iter()
                .zip(&installed)
                .filter(|(before, after)| before != after)
                .count(),
            335
        );
        assert_eq!(
            project
                .load_title_recording_detected(&locator)
                .unwrap()
                .recording,
            Some(recording.clone())
        );
        let exported = encode_zsnes_title_recording(&recording);
        assert_eq!(exported.len(), 134_163);
        assert_eq!(
            format!("{:x}", Sha256::digest(&exported)),
            "958059ec938e651410f01f6b692176c5037adc854f4fc218bbd051de782f0964"
        );
        let observation = include_str!(
            "../../../docs/oracle-work/lm363/smw-us-lorom/title-playback-import/observation.tsv"
        );
        for required in [
            "playback_import_command\t0x1F44",
            "output_rom_sha256\t758c41d8f849d2a96efa76f789f471b37e2843981f0b759e34c6a670cc936676",
            "changed_bytes\t335",
            "confirmation_cancel_byte_identical\ttrue",
            "file_dialog_cancel_byte_identical\ttrue",
            "batch_export_byte_identical_to_input\ttrue",
            "batch_malformed_rom_byte_identical\ttrue",
            "batch_absent_export_created_output\tfalse",
        ] {
            assert!(observation.lines().any(|line| line == required));
        }
    }
}
