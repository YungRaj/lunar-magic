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
    use lm_rom::{CopierHeader, RomImage, detect_identity};
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
    fn event_movement_round_trips_every_storage_and_copier_header_variant() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixtures =
            [
                (false, crate::test_support::pristine_smw_us_rom_bytes()),
                (
                    true,
                    fs::read(root.join(
                        "oracle-work/lm363/pristine-us/overworld-transfer-positive/after.smc",
                    ))
                    .unwrap(),
                ),
            ];
        let locator = smw_us_v1_overworld_event_reveal_locator();

        for (transferred, fixture) in fixtures {
            let logical = RomImage::from_bytes(fixture)
                .unwrap()
                .logical_bytes()
                .to_vec();
            let mut headered = RomImage::from_bytes(logical.clone()).unwrap();
            headered.set_copier_header(CopierHeader::Present, 0xa5);
            let variants = [logical, headered.as_file_bytes().to_vec()];
            let mut logical_results = Vec::new();

            for original in variants {
                let original_image = RomImage::from_bytes(original.clone()).unwrap();
                let original_header = original_image.copier_header_bytes().map(<[u8]>::to_vec);
                let mut project = Project::open_supported(original_image).unwrap();
                let loaded = project
                    .load_overworld_event_reveals_detected(locator)
                    .unwrap();
                if transferred {
                    assert!(matches!(
                        loaded.storage,
                        OverworldEventRevealStorage::TransferredSources { .. }
                    ));
                } else {
                    assert_eq!(loaded.storage, OverworldEventRevealStorage::Fixed);
                }

                let (selected, mut moved, displacement) = (0..loaded.table.entries.len())
                    .find_map(|selected| {
                        let mut candidate = loaded.table.clone();
                        candidate
                            .relocate_selection(&[selected], 1, 1)
                            .ok()
                            .flatten()
                            .map(|displacement| (selected, candidate, displacement))
                    })
                    .expect("fixture must contain at least one movable event reveal");
                project
                    .save_overworld_event_reveals_detected(
                        &moved,
                        locator,
                        &smw_us_v1_overworld_event_allocation_policy(),
                        crate::SMW_US_V1_CHECKSUM_FIELD,
                        0xff,
                    )
                    .unwrap();
                let first_save = project.save_snapshot();
                assert!(matches!(
                    project
                        .load_overworld_event_reveals_detected(locator)
                        .unwrap()
                        .storage,
                    OverworldEventRevealStorage::Expanded { .. }
                ));

                assert_eq!(
                    moved
                        .relocate_selection(&[selected], -displacement.0, -displacement.1)
                        .unwrap(),
                    Some((-displacement.0, -displacement.1))
                );
                project
                    .save_overworld_event_reveals_detected(
                        &moved,
                        locator,
                        &smw_us_v1_overworld_event_allocation_policy(),
                        crate::SMW_US_V1_CHECKSUM_FIELD,
                        0xff,
                    )
                    .unwrap();
                let second_save = project.save_snapshot();
                assert_eq!(
                    project
                        .load_overworld_event_reveals_detected(locator)
                        .unwrap()
                        .table,
                    moved
                );
                let result = RomImage::from_bytes(second_save.clone()).unwrap();
                assert_eq!(
                    result.copier_header_bytes().map(<[u8]>::to_vec),
                    original_header
                );
                assert!(detect_identity(&result).unwrap().checksum_matches());
                logical_results.push(result.logical_bytes().to_vec());

                assert!(project.undo().unwrap());
                assert_eq!(project.save_snapshot(), first_save);
                assert!(project.undo().unwrap());
                assert_eq!(project.save_snapshot(), original);
                assert!(project.redo().unwrap());
                assert_eq!(project.save_snapshot(), first_save);
                assert!(project.redo().unwrap());
                assert_eq!(project.save_snapshot(), second_save);
            }

            assert_eq!(logical_results[0], logical_results[1]);
        }
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
