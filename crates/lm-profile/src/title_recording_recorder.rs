//! SMW US revision-0 temporary title-movement recorder runtime.

use lm_project::{TitleRecordingExpansionWrite, TitleRecordingRecorderLocator};
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_TITLE_RECORDER_FIRST_HOOK_OFFSET: usize = 0x0000_21da;
pub const SMW_US_V1_TITLE_RECORDER_SECOND_HOOK_OFFSET: usize = 0x0002_d79b;
pub const SMW_US_V1_TITLE_RECORDER_SEARCH_START: usize = 0x0008_6e30;
pub const SMW_US_V1_TITLE_RECORDER_COMPENSATION_OFFSET: usize = 0x0007_efa3;
pub const SMW_US_V1_TITLE_RECORDER_COMPENSATION_LEN: usize = 0xeb;

const RUNTIME: [u8; 0xb2] = [
    0x08, 0xa5, 0x15, 0x8f, 0xfb, 0xff, 0x7f, 0xa5, 0x16, 0x29, 0xc0, 0x4a, 0x4a, 0x4a, 0x4a, 0x8f,
    0xfa, 0xff, 0x7f, 0xa5, 0x17, 0x29, 0xb0, 0x0f, 0xfa, 0xff, 0x7f, 0x8f, 0xfa, 0xff, 0x7f, 0xa5,
    0x18, 0x29, 0x80, 0x4a, 0x0f, 0xfa, 0xff, 0x7f, 0x8f, 0xfa, 0xff, 0x7f, 0xc2, 0x30, 0xa9, 0x42,
    0x00, 0xcf, 0xfc, 0xff, 0x7f, 0xf0, 0x0a, 0x8f, 0xfc, 0xff, 0x7f, 0xa9, 0x00, 0x00, 0xaa, 0x80,
    0x2e, 0xaf, 0xf8, 0xff, 0x7f, 0xaa, 0xe2, 0x20, 0xbf, 0x00, 0x00, 0x7f, 0xcf, 0xfb, 0xff, 0x7f,
    0xd0, 0x17, 0xbf, 0x01, 0x00, 0x7f, 0xcf, 0xfa, 0xff, 0x7f, 0xd0, 0x0d, 0xbf, 0x02, 0x00, 0x7f,
    0xf0, 0x07, 0x1a, 0x9f, 0x02, 0x00, 0x7f, 0x80, 0x28, 0xe8, 0xe8, 0xe8, 0xc2, 0x20, 0x8a, 0x8f,
    0xf8, 0xff, 0x7f, 0xe2, 0x20, 0xaf, 0xfb, 0xff, 0x7f, 0x9f, 0x00, 0x00, 0x7f, 0xaf, 0xfa, 0xff,
    0x7f, 0x9f, 0x01, 0x00, 0x7f, 0xa9, 0x01, 0x9f, 0x02, 0x00, 0x7f, 0xa9, 0xff, 0x9f, 0x03, 0x00,
    0x7f, 0x28, 0xad, 0x26, 0x14, 0xf0, 0x08, 0x48, 0x22, 0x0c, 0xb1, 0x05, 0x68, 0xc9, 0x00, 0x6b,
    0x08, 0xc2, 0x20, 0xa9, 0x00, 0x00, 0x8f, 0xfc, 0xff, 0x7f, 0x28, 0x9c, 0xcf, 0x13, 0xad, 0x95,
    0x1b, 0x6b,
];

const EXPANSION_METADATA_PADDING: [u8; 0x12] = [0; 0x12];
const EXPANSION_WRITES: [TitleRecordingExpansionWrite; 3] = [
    TitleRecordingExpansionWrite {
        offset: 0x0007_f08e,
        bytes: &EXPANSION_METADATA_PADDING,
    },
    TitleRecordingExpansionWrite {
        offset: 0x0007_f0a0,
        bytes: &crate::title_recording::EXPANSION_ATTRIBUTION,
    },
    TitleRecordingExpansionWrite {
        offset: 0x0007_ffe7,
        bytes: &crate::title_recording::EXPANSION_FEATURE_RECORD,
    },
];

#[must_use]
pub fn smw_us_v1_title_recording_recorder_locator() -> TitleRecordingRecorderLocator {
    TitleRecordingRecorderLocator {
        mapper: Mapper::LoRom,
        first_hook: SMW_US_V1_TITLE_RECORDER_FIRST_HOOK_OFFSET,
        pristine_first_hook: vec![0xad, 0x26, 0x14, 0xf0, 0x05, 0x22, 0x0c, 0xb1, 0x05, 0x60],
        installed_first_hook: vec![0x22, 0, 0, 0, 0xf0, 0x04, 0x60, 0xea, 0xea, 0xea],
        first_hook_pointer: 1,
        second_hook: SMW_US_V1_TITLE_RECORDER_SECOND_HOOK_OFFSET,
        pristine_second_hook: vec![0x9c, 0xcf, 0x13, 0xad, 0x95, 0x1b],
        installed_second_hook: vec![0x22, 0, 0, 0, 0xea, 0xea],
        second_hook_pointer: 1,
        second_runtime_offset: 0xa0,
        runtime_template: RUNTIME.to_vec(),
        rom_size_field: Some(0x0000_7fd7),
        expansion_writes: &EXPANSION_WRITES,
        compensation: SMW_US_V1_TITLE_RECORDER_COMPENSATION_OFFSET,
        compensation_len: SMW_US_V1_TITLE_RECORDER_COMPENSATION_LEN,
        checksum_field: crate::SMW_US_V1_CHECKSUM_FIELD,
    }
}

#[must_use]
pub fn smw_us_v1_title_recording_recorder_allocation_policy(image_len: usize) -> AllocationPolicy {
    let search_start = if image_len <= 0x08_0000 {
        0x08_0000
    } else {
        SMW_US_V1_TITLE_RECORDER_SEARCH_START
    };
    AllocationPolicy::lorom(search_start..image_len.max(0x10_0000))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::TitleRecordingRecorderState;
    use lm_rom::RomImage;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    #[test]
    fn install_matches_retained_lunar_magic_oracle_and_uninstall_is_reciprocal() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(original) = std::fs::read(root.join("Super Mario World (USA).sfc")) else {
            return;
        };
        if format!("{:x}", Sha256::digest(&original))
            != "7300346506c982766ed3ae370c56a31e30ad7a9603706bc3c6b18051e70f41c7"
        {
            return;
        }
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let locator = smw_us_v1_title_recording_recorder_locator();
        let policy =
            smw_us_v1_title_recording_recorder_allocation_policy(project.rom.logical_len());
        assert_eq!(
            project
                .load_title_recording_recorder_detected(&locator)
                .unwrap(),
            TitleRecordingRecorderState::Absent
        );
        assert!(
            project
                .install_title_recording_recorder(&locator, &policy)
                .unwrap()
        );
        let installed = project.save_snapshot();
        assert_eq!(
            format!("{:x}", Sha256::digest(&installed)),
            "abc3977c5e03535fa0b60ad6339e231e600a59759b0b6d77228dc840c07f3b9b"
        );
        assert_eq!(
            original
                .iter()
                .zip(&installed)
                .filter(|(before, after)| before != after)
                .count(),
            347
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
        project.redo().unwrap();
        assert_eq!(project.save_snapshot(), installed);
        assert!(
            project
                .uninstall_title_recording_recorder(&locator, &policy)
                .unwrap()
        );
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn canonical_vanilla_install_expands_reopens_and_undoes_atomically() {
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let locator = smw_us_v1_title_recording_recorder_locator();
        let policy =
            smw_us_v1_title_recording_recorder_allocation_policy(project.rom.logical_len());
        project
            .install_title_recording_recorder(&locator, &policy)
            .unwrap();
        assert_eq!(project.rom.logical_len(), 0x10_0000);
        assert!(matches!(
            project
                .load_title_recording_recorder_detected(&locator)
                .unwrap(),
            TitleRecordingRecorderState::Installed { .. }
        ));
        assert!(
            lm_rom::detect_identity(&project.rom)
                .unwrap()
                .checksum_matches()
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(project.save_snapshot())),
            "663f824b807c8addc81be50b35cd6d2b5f714427063107ddc52aa037c962341f"
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn retained_oracle_binds_commands_dialogs_success_cancel_and_reciprocal_removal() {
        let observation = include_str!(
            "../../../docs/oracle-work/lm363/smw-us-lorom/title-recording-recorder/observation.tsv"
        );
        for required in [
            "main_open_overworld_command\t0x232D",
            "overworld_save_command\t0x1F40",
            "install_recorder_command\t0x1F46",
            "uninstall_recorder_command\t0x1F47",
            "install_dialog_title\tWarning: Install Joypad Recorder for Levels?",
            "installed_changed_bytes\t347",
            "installed_sha256\tabc3977c5e03535fa0b60ad6339e231e600a59759b0b6d77228dc840c07f3b9b",
            "uninstall_exactly_reciprocal\ttrue",
            "cancel_byte_identical\ttrue",
        ] {
            assert!(observation.lines().any(|line| line == required));
        }
    }
}
