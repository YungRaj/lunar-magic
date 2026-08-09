//! SMW US revision-0 title-screen playback runtime metadata.

use lm_project::{TitleRecordingExpansionWrite, TitleRecordingPatchLocator};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_TITLE_RECORDING_HOOK_OFFSET: usize = 0x0000_1c6f;
pub const SMW_US_V1_TITLE_RECORDING_SEARCH_START: usize = 0x0006_abf7;
pub const SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL: u8 = 0x00;

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

pub(crate) const EXPANSION_ATTRIBUTION: [u8; 0xa0] =
    *b"Lunar Magic Version 3.63 Public \xa92025 FuSoYa, Defender of Relm http://fusoya.eludevisibility.org                                I am Naaall, and I love fiiiish!";
pub(crate) const EXPANSION_FEATURE_RECORD: [u8; 0x19] = [
    0x00, 0x00, 0xf8, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0xff, 0x00, 0x02, 0x10, 0x00,
    0x02, 0x08, 0x00, 0x02, 0x04, 0x00, 0x02, 0x08, 0x00,
];
pub(crate) const EXPANSION_WRITES: [TitleRecordingExpansionWrite; 2] = [
    TitleRecordingExpansionWrite {
        offset: 0x0007_f0a0,
        bytes: &EXPANSION_ATTRIBUTION,
    },
    TitleRecordingExpansionWrite {
        offset: 0x0007_ffe7,
        bytes: &EXPANSION_FEATURE_RECORD,
    },
];

#[must_use]
pub const fn smw_us_v1_title_recording_locator() -> TitleRecordingPatchLocator {
    TitleRecordingPatchLocator {
        mapper: Mapper::LoRom,
        hook: SMW_US_V1_TITLE_RECORDING_HOOK_OFFSET,
        pristine_hook: PRISTINE_HOOK,
        hook_template: HOOK_TEMPLATE,
        runtime_template: RUNTIME_TEMPLATE,
        rom_size_field: Some(0x0000_7fd7),
        expansion_writes: &EXPANSION_WRITES,
        checksum_compensation: Some(0x0007_efa3..0x0007_f0a0),
    }
}

#[must_use]
pub fn smw_us_v1_title_recording_allocation_policy(image_len: usize) -> AllocationPolicy {
    let search_end = if image_len <= 0x08_0000 {
        // Lunar Magic's "Not enough room" path expands a vanilla ROM to the next supported
        // product size rather than appending only the bank needed by this small payload.
        0x10_0000
    } else {
        image_len.saturating_add(0x8000).min(0x40_0000)
    };
    let mut policy =
        AllocationPolicy::lorom(SMW_US_V1_TITLE_RECORDING_SEARCH_START.max(0x08_0000)..search_end);
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
                SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
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
                SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
            )
            .unwrap();
        assert_eq!(
            project
                .load_title_recording_detected(&locator)
                .unwrap()
                .recording,
            Some(second)
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(project.save_snapshot())),
            "e5a23569110361c6a676487932d57d4aa0e9537d3bbf4ff5a3ac957d193a1e6c"
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
                SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
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
            "vanilla_output_sha256\t662f1f980bb02f8ec2f6ac1be27835f7269091336f0f07008499afe6717c058c",
            "vanilla_rust_byte_identical\ttrue",
            "vanilla_update_output_sha256\t46079b7e14c90d89cc7b46a797bd05a48fabacaec7fc6d7e63134bc405d36bb0",
            "vanilla_update_rust_byte_identical\ttrue",
        ] {
            assert!(observation.lines().any(|line| line == required));
        }
    }

    #[test]
    fn vanilla_playback_import_expands_and_matches_lunar_magic_byte_for_byte() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        assert_eq!(original.len(), 0x08_0000);
        let recording = TitleScreenRecording::from_bytes(vec![0x12, 0x34, 0x56, 0xff]).unwrap();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        project
            .save_title_recording_detected(
                &recording,
                &smw_us_v1_title_recording_locator(),
                &smw_us_v1_title_recording_allocation_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                SMW_US_V1_TITLE_RECORDING_RECLAIM_FILL,
            )
            .unwrap();
        let installed = project.save_snapshot();
        assert_eq!(installed.len(), 0x10_0000);
        assert_eq!(installed[0x7fd7], 0x0a);
        assert_eq!(
            format!("{:x}", Sha256::digest(&installed)),
            "6b1be7f9ff80479d40a6746a1daad56b0b56f8967b82a4232d910a9cbd1facdd"
        );
        assert_eq!(
            project
                .load_title_recording_detected(&smw_us_v1_title_recording_locator())
                .unwrap()
                .recording,
            Some(recording)
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }
}
