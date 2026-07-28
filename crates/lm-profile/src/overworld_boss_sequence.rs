//! Lunar Magic-compatible boss-sequence message storage for SMW US revision 0.

use lm_project::BossSequencePatchLocator;
use lm_rats::AllocationPolicy;
use lm_rom::Mapper;

pub const SMW_US_V1_BOSS_SEQUENCE_FIRST_POINTER: usize = 0x04f1;
pub const SMW_US_V1_BOSS_SEQUENCE_SEARCH_START: usize = 0x08_0000;
pub const SMW_US_V1_BOSS_SEQUENCE_SEARCH_END: usize = 0x09_0000;

#[must_use]
pub const fn smw_us_v1_boss_sequence_locator() -> BossSequencePatchLocator {
    BossSequencePatchLocator {
        mapper: Mapper::LoRom,
        first_pointer: SMW_US_V1_BOSS_SEQUENCE_FIRST_POINTER,
    }
}

#[must_use]
pub fn smw_us_v1_boss_sequence_allocation_policy() -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_BOSS_SEQUENCE_SEARCH_START..SMW_US_V1_BOSS_SEQUENCE_SEARCH_END,
    )
}

#[must_use]
pub fn smw_us_v1_boss_sequence_update_policy(image_len: usize) -> AllocationPolicy {
    AllocationPolicy::lorom(
        SMW_US_V1_BOSS_SEQUENCE_SEARCH_START..image_len.saturating_add(0x8000).min(0x40_0000),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SMW_US_V1_CHECKSUM_FIELD;
    use lm_overworld::BossSequenceMessage;
    use lm_rom::RomImage;
    use std::path::PathBuf;

    #[test]
    fn pristine_rows_save_as_one_combined_allocation_and_undo_exactly() {
        let _root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = crate::test_support::pristine_smw_us_rom_bytes();
        let mut project =
            lm_project::Project::open_supported(RomImage::from_bytes(original.clone()).unwrap())
                .unwrap();
        let mut loaded = project
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap();
        loaded.table.messages[6] = BossSequenceMessage([0x1f; 192]);
        loaded.table.messages[6].0[0] = 0x2a;
        project
            .save_boss_sequence_messages_detected(
                &loaded.table,
                smw_us_v1_boss_sequence_locator(),
                &smw_us_v1_boss_sequence_allocation_policy(),
                SMW_US_V1_CHECKSUM_FIELD,
                0xff,
            )
            .unwrap();
        let reopened = project
            .load_boss_sequence_messages_detected(smw_us_v1_boss_sequence_locator())
            .unwrap();
        assert_eq!(reopened.table, loaded.table);
        assert!(matches!(
            reopened.storage,
            lm_project::BossSequenceStorage::Combined(_)
        ));
        project.undo().unwrap();
        assert_eq!(project.save_snapshot(), original);
    }
}
