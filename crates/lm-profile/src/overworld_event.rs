//! SMW US revision-0 main overworld event-reveal pointer layout.

use lm_project::OverworldEventRevealLocator;
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_OVERWORLD_EVENT_SOURCE_OPERAND_OFFSET: usize = 0x02_5a74;
pub const SMW_US_V1_OVERWORLD_EVENT_DESTINATION_OPERAND_OFFSET: usize = 0x02_5a84;
pub const SMW_US_V1_OVERWORLD_EVENT_FIXED_ENTRIES: usize = 112;
pub const SMW_US_V1_OVERWORLD_EVENT_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_OVERWORLD_EVENT_SEARCH_END: usize = 0x09_0000;

#[must_use]
pub const fn smw_us_v1_overworld_event_reveal_locator() -> OverworldEventRevealLocator {
    OverworldEventRevealLocator {
        mapper: Mapper::LoRom,
        source_operand_offset: SMW_US_V1_OVERWORLD_EVENT_SOURCE_OPERAND_OFFSET,
        destination_operand_offset: SMW_US_V1_OVERWORLD_EVENT_DESTINATION_OPERAND_OFFSET,
        fixed_entries: SMW_US_V1_OVERWORLD_EVENT_FIXED_ENTRIES,
    }
}

#[must_use]
pub fn smw_us_v1_overworld_event_allocation_policy() -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_OVERWORLD_EVENT_SEARCH_START..SMW_US_V1_OVERWORLD_EVENT_SEARCH_END,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::EventReveal;
    use lm_project::{OverworldEventRevealPatchError, OverworldEventRevealStorage, Project};
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    #[test]
    fn pristine_loads_and_expanded_growth_reopens_and_undoes() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            Project::open_supported(RomImage::from_bytes(original.clone()).unwrap()).unwrap();
        let loaded = project
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap();
        assert_eq!(loaded.table.entries.len(), 112);
        assert_eq!(loaded.storage, OverworldEventRevealStorage::Fixed);
        let table = lm_overworld::EventRevealTable {
            entries: (0..200)
                .map(|index| EventReveal {
                    source_tile: index,
                    destination_tile: index | 0x200,
                })
                .collect(),
        };
        project
            .save_overworld_event_reveals_detected(
                &table,
                smw_us_v1_overworld_event_reveal_locator(),
                &smw_us_v1_overworld_event_allocation_policy(),
                crate::SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        let reopened = project
            .load_overworld_event_reveals_detected(smw_us_v1_overworld_event_reveal_locator())
            .unwrap();
        assert_eq!(reopened.table, table);
        assert!(matches!(
            reopened.storage,
            OverworldEventRevealStorage::Expanded { .. }
        ));
        assert!(project.undo().unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn lunar_magic_transfer_uses_tagged_sources_and_fixed_destinations() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let before = fs::read(
            root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/before.smc"),
        )
        .unwrap();
        let after_bytes = fs::read(
            root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc"),
        )
        .unwrap();
        let before = Project::open_supported(RomImage::from_bytes(before).unwrap()).unwrap();
        let after =
            Project::open_supported(RomImage::from_bytes(after_bytes.clone()).unwrap()).unwrap();
        let locator = smw_us_v1_overworld_event_reveal_locator();
        let before = before
            .load_overworld_event_reveals_detected(locator)
            .unwrap();
        let after = after
            .load_overworld_event_reveals_detected(locator)
            .unwrap();
        assert_eq!(before.table.entries.len(), 112);
        assert_eq!(after.table.entries.len(), 120);
        assert!(
            before
                .table
                .entries
                .iter()
                .zip(&after.table.entries)
                .all(|(before, after)| before.source_tile == after.source_tile)
        );
        assert!(
            after.table.entries[112..]
                .iter()
                .all(|entry| entry.source_tile == 0)
        );
        assert!(matches!(
            after.storage,
            OverworldEventRevealStorage::TransferredSources { .. }
        ));

        let mut editable =
            Project::open_supported(RomImage::from_bytes(after_bytes.clone()).unwrap()).unwrap();
        let mut changed = editable
            .load_overworld_event_reveals_detected(locator)
            .unwrap()
            .table;
        changed.entries[0].destination_tile ^= 1;
        editable
            .save_overworld_event_reveals_detected(
                &changed,
                locator,
                &smw_us_v1_overworld_event_allocation_policy(),
                crate::SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        let reopened = editable
            .load_overworld_event_reveals_detected(locator)
            .unwrap();
        assert_eq!(reopened.table, changed);
        assert!(matches!(
            reopened.storage,
            OverworldEventRevealStorage::Expanded { .. }
        ));
        assert!(editable.undo().unwrap());
        assert_eq!(editable.save_snapshot(), after_bytes);
    }

    #[test]
    fn malformed_hybrid_event_storage_is_rejected() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("oracle-work/lm363/pristine-us/overworld-transfer-positive");
        let before = fs::read(fixture.join("before.smc")).unwrap();
        let after = fs::read(fixture.join("after.smc")).unwrap();
        let locator = smw_us_v1_overworld_event_reveal_locator();

        let mut odd = after.clone();
        let header = 0x08_48f2 + 0x200;
        assert_eq!(&odd[header..header + 8], b"STAR\xef\x00\x10\xff");
        odd[header + 4..header + 8].copy_from_slice(&[0xee, 0x00, 0x11, 0xff]);
        let odd = Project::open_supported(RomImage::from_bytes(odd).unwrap()).unwrap();
        assert!(matches!(
            odd.load_overworld_event_reveals_detected(locator),
            Err(OverworldEventRevealPatchError::PlaneLength { .. })
        ));

        let mut destination_only = after;
        let header_len = 0x200;
        let source = locator.source_operand_offset + header_len;
        let destination = locator.destination_operand_offset + header_len;
        let transferred_pointer: [u8; 3] = destination_only[source..source + 3].try_into().unwrap();
        destination_only[destination..destination + 3].copy_from_slice(&transferred_pointer);
        destination_only[source..source + 3].copy_from_slice(&before[source..source + 3]);
        let destination_only =
            Project::open_supported(RomImage::from_bytes(destination_only).unwrap()).unwrap();
        assert!(matches!(
            destination_only.load_overworld_event_reveals_detected(locator),
            Err(OverworldEventRevealPatchError::MixedStorage)
        ));
    }
}
