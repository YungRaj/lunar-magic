//! SMW US revision-0 overworld event-number mapping runtime and storage.

use lm_project::OverworldEventNumberMapLocator;
use lm_rom::Mapper;

pub const SMW_US_V1_EVENT_NUMBER_LEGACY_PROBE_OFFSET: usize = 0x02_57f9;
pub const SMW_US_V1_EVENT_NUMBER_LEGACY_PAIRS_OFFSET: usize = 0x00_1ee0;
pub const SMW_US_V1_EVENT_NUMBER_HOOK_OFFSET: usize = 0x00_1f19;
pub const SMW_US_V1_EVENT_NUMBER_RUNTIME_OFFSET: usize = 0x02_dd80;
pub const SMW_US_V1_EVENT_NUMBER_FIXED_MAP_OFFSET: usize = 0x02_dda0;
pub const SMW_US_V1_EVENT_NUMBER_EXTENDED_MAP_OFFSET: usize = 0x01_be80;

pub const SMW_US_V1_EVENT_NUMBER_RUNTIME: [u8; 32] = [
    0x08, 0xc2, 0x30, 0xa2, 0x5e, 0x00, 0xbf, 0xa0, 0xdd, 0x05, 0x9f, 0x49, 0x1f, 0x00, 0xca, 0xca,
    0x10, 0xf4, 0x28, 0x6b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4c, 0x4d, 0x10, 0x01,
];

/// Lunar Magic emits the low-bank `LoROM` mirror for fixed executable code and tables.
///
/// Both `$05:xxxx` and `$85:xxxx` resolve to the same ROM bytes, but retaining the exact
/// low-bank spelling is required for byte-faithful detection of Lunar Magic's runtime.
pub const SMW_US_V1_EVENT_NUMBER_POINTER_BANK_MASK: u8 = 0x7f;

#[must_use]
pub const fn smw_us_v1_overworld_event_number_map_locator() -> OverworldEventNumberMapLocator {
    OverworldEventNumberMapLocator {
        mapper: Mapper::LoRom,
        legacy_probe_offset: SMW_US_V1_EVENT_NUMBER_LEGACY_PROBE_OFFSET,
        legacy_fixed_opcode: 0xa2,
        legacy_pairs_offset: SMW_US_V1_EVENT_NUMBER_LEGACY_PAIRS_OFFSET,
        legacy_pairs_len: 0x10,
        hook_offset: SMW_US_V1_EVENT_NUMBER_HOOK_OFFSET,
        pristine_hook: [0xca, 0xca, 0x10, 0xf3],
        runtime_offset: SMW_US_V1_EVENT_NUMBER_RUNTIME_OFFSET,
        runtime_template: SMW_US_V1_EVENT_NUMBER_RUNTIME,
        runtime_pointer_operand: 7,
        fixed_map_offset: SMW_US_V1_EVENT_NUMBER_FIXED_MAP_OFFSET,
        extended_map_offset: SMW_US_V1_EVENT_NUMBER_EXTENDED_MAP_OFFSET,
        pointer_bank_mask: SMW_US_V1_EVENT_NUMBER_POINTER_BANK_MASK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::EventNumberMap;
    use lm_project::{OverworldEventNumberMapStorage, Project};
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_pairs_install_extended_shrink_and_undo_exactly() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let locator = smw_us_v1_overworld_event_number_map_locator();
        let pristine = project
            .load_overworld_event_number_map_detected(locator)
            .unwrap();
        assert_eq!(
            pristine.storage,
            OverworldEventNumberMapStorage::LegacyPairs
        );
        assert_eq!(pristine.map.stored_len(), EventNumberMap::VANILLA_LEN);
        assert_eq!(pristine.map.get(0x28), 3);
        assert_eq!(pristine.map.get(0x5b), 8);

        let mut extended = pristine.map;
        extended.set(0xff, 0x7e);
        project
            .save_overworld_event_number_map_detected(
                &extended,
                locator,
                crate::SMW_US_V1_CHECKSUM_FIELD,
            )
            .unwrap();
        let reopened = project
            .load_overworld_event_number_map_detected(locator)
            .unwrap();
        assert_eq!(reopened.map, extended);
        assert_eq!(
            reopened.storage,
            OverworldEventNumberMapStorage::InstalledExtended
        );
        assert!(project.identity.as_ref().unwrap().checksum_matches());

        let compact =
            EventNumberMap::decode(&extended.encode()[..EventNumberMap::VANILLA_LEN]).unwrap();
        project
            .save_overworld_event_number_map_detected(
                &compact,
                locator,
                crate::SMW_US_V1_CHECKSUM_FIELD,
            )
            .unwrap();
        assert_eq!(
            project
                .load_overworld_event_number_map_detected(locator)
                .unwrap()
                .storage,
            OverworldEventNumberMapStorage::InstalledFixed
        );
        assert!(project.undo().unwrap());
        assert_eq!(
            project
                .load_overworld_event_number_map_detected(locator)
                .unwrap()
                .map,
            extended
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn wine_transfer_overworld_event_map_reopens_and_updates_exactly() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let locator = smw_us_v1_overworld_event_number_map_locator();
        let pristine = Project::open_supported(RomImage::from_bytes(before).unwrap()).unwrap();
        let pristine_map = pristine
            .load_overworld_event_number_map_detected(locator)
            .unwrap()
            .map;
        let mut project =
            Project::open_supported(RomImage::from_bytes(after.clone()).unwrap()).unwrap();
        let loaded = project
            .load_overworld_event_number_map_detected(locator)
            .unwrap();
        assert_eq!(loaded.map, pristine_map);
        assert_eq!(
            loaded.storage,
            OverworldEventNumberMapStorage::InstalledFixed
        );

        let mut edited = loaded.map;
        edited.set(0x5f, 0x7e);
        project
            .save_overworld_event_number_map_detected(
                &edited,
                locator,
                crate::SMW_US_V1_CHECKSUM_FIELD,
            )
            .unwrap();
        assert_eq!(
            project
                .load_overworld_event_number_map_detected(locator)
                .unwrap()
                .map,
            edited
        );
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), after);

        project
            .rom
            .write(SMW_US_V1_EVENT_NUMBER_HOOK_OFFSET + 3, &[0x85])
            .unwrap();
        assert!(matches!(
            project.load_overworld_event_number_map_detected(locator),
            Err(lm_project::OverworldEventNumberMapError::Hook(_))
        ));
    }
}
