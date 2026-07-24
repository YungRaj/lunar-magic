//! SMW US revision-0 Lunar Magic expanded secondary-exit readers.

use lm_project::SecondaryExitPatchLocator;
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_SECONDARY_EXIT_FIRST_READER: usize = 0x0006_e190;
pub const SMW_US_V1_SECONDARY_EXIT_SECOND_READER: usize = 0x0002_dc80;
pub const SMW_US_V1_SECONDARY_EXIT_FIXED_PLANES: [usize; 4] =
    [0x0002_f800, 0x0002_fa00, 0x0002_fc00, 0x0002_fe00];
pub const SMW_US_V1_SECONDARY_EXIT_SEARCH_START: usize = 0x0008_0000;

#[must_use]
pub const fn smw_us_v1_secondary_exit_locator() -> SecondaryExitPatchLocator {
    SecondaryExitPatchLocator {
        mapper: Mapper::LoRom,
        first_reader: SMW_US_V1_SECONDARY_EXIT_FIRST_READER,
        second_reader: SMW_US_V1_SECONDARY_EXIT_SECOND_READER,
        fixed_planes: SMW_US_V1_SECONDARY_EXIT_FIXED_PLANES,
    }
}

#[must_use]
pub fn smw_us_v1_secondary_exit_allocation_policy(image_len: usize) -> AllocationPolicy {
    AllocationPolicy::lorom(SMW_US_V1_SECONDARY_EXIT_SEARCH_START..image_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_project::{Project, SecondaryExitStorage};
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn real_lm363_table_loads_updates_reopens_and_undoes_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let locator = smw_us_v1_secondary_exit_locator();
        let loaded = project.load_secondary_exit_table_detected(locator).unwrap();
        assert!(matches!(
            loaded.storage,
            SecondaryExitStorage::Installed {
                fixed_prefix_planes: 4,
                used_len: 0x1fe,
                ref tagged_planes,
            } if tagged_planes.len() == 2
        ));
        let mut edited = loaded.table;
        edited.entries[0x123].destination_level = 0x105;
        edited.entries[0x123].position_and_method = 0x21;
        project
            .save_installed_secondary_exit_table(
                &edited,
                locator,
                &smw_us_v1_secondary_exit_allocation_policy(project.rom.logical_len()),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        assert_eq!(
            project
                .load_secondary_exit_table_detected(locator)
                .unwrap()
                .table,
            edited
        );
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn pristine_rom_loads_four_native_planes_without_claiming_installation() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read(root.join("Super Mario World (USA).sfc")).unwrap();
        let project = Project::open_supported(RomImage::from_bytes(original).unwrap()).unwrap();
        let loaded = project
            .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
            .unwrap();
        assert_eq!(loaded.storage, SecondaryExitStorage::Pristine);
        assert_eq!(loaded.table.entries.len(), 0x2000);
    }

    #[test]
    fn independent_lm363_operations_resolve_different_owners_to_the_same_table() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut expected = None;
        for fixture in [
            "level-save-000",
            "level-save-105",
            "palette-install-positive",
            "exanimation-install-positive",
        ] {
            let bytes =
                fs::read(root.join(format!("oracle-work/lm363/pristine-us/{fixture}/after.smc")))
                    .unwrap();
            let project = Project::open_supported(RomImage::from_bytes(bytes).unwrap()).unwrap();
            let loaded = project
                .load_secondary_exit_table_detected(smw_us_v1_secondary_exit_locator())
                .unwrap();
            assert!(matches!(
                loaded.storage,
                SecondaryExitStorage::Installed { .. }
            ));
            if let Some(expected) = &expected {
                assert_eq!(&loaded.table, expected);
            } else {
                expected = Some(loaded.table);
            }
        }
    }
}
