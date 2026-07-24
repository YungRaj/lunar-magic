//! SMW US revision-0 native overworld player-start layout.

use lm_project::OverworldPlayerStartRomLayout;
use lm_rom::Mapper;

pub const SMW_US_V1_OVERWORLD_PLAYER_START_OPTIONS_OFFSET: usize = 0x00_1ef0;
pub const SMW_US_V1_OVERWORLD_CUSTOM_START_PATCH_OFFSET: usize = 0x02_b15d;
pub const SMW_US_V1_OVERWORLD_CUSTOM_START_PRISTINE: [u8; 3] = [0x8d, 0x19, 0x1f];
pub const SMW_US_V1_OVERWORLD_CUSTOM_START_ENABLED: [u8; 3] = [0xea; 3];

#[must_use]
pub const fn smw_us_v1_overworld_player_start_layout() -> OverworldPlayerStartRomLayout {
    OverworldPlayerStartRomLayout {
        mapper: Mapper::LoRom,
        options_offset: SMW_US_V1_OVERWORLD_PLAYER_START_OPTIONS_OFFSET,
        custom_start_patch_offset: SMW_US_V1_OVERWORLD_CUSTOM_START_PATCH_OFFSET,
        pristine_patch: SMW_US_V1_OVERWORLD_CUSTOM_START_PRISTINE,
        enabled_patch: SMW_US_V1_OVERWORLD_CUSTOM_START_ENABLED,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_starts_round_trip_and_custom_change_is_checksum_valid_and_undoable() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let layout = smw_us_v1_overworld_player_start_layout();
        let mut starts = project.load_overworld_player_starts(layout).unwrap();
        assert!(starts.is_vanilla());
        assert_eq!(starts.encode().unwrap(), original[0x1ef0..0x1f06]);
        starts.starts[1].x = 0x88;
        assert!(
            project
                .save_overworld_player_starts(&starts, layout, SMW_US_V1_CHECKSUM_FIELD)
                .unwrap()
        );
        assert_eq!(
            project.load_overworld_player_starts(layout).unwrap(),
            starts
        );
        assert_eq!(
            project
                .rom
                .read(SMW_US_V1_OVERWORLD_CUSTOM_START_PATCH_OFFSET, 3)
                .unwrap(),
            &[0xea; 3]
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }
}
