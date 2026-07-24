use crate::{
    PayloadLoadError, PayloadPointer, PayloadSaveError, PayloadSaveRequest, PayloadSaveResult,
    Project,
};
use lm_graphics::{
    EXANIMATION_LEVEL_SLOT_COUNT, ExAnimationSlotOptionError, ExAnimationSlotOptionTable,
};
use lm_rats::{AllocationPolicy, RatsBlock};
use lm_rom::Mapper;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExAnimationSlotOptionRomLayout {
    pub mapper: Mapper,
    pub pointer_offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedExAnimationSlotOptions {
    pub table: ExAnimationSlotOptionTable,
    pub block: RatsBlock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExAnimationSlotOptionSaveOptions {
    pub allocation: AllocationPolicy,
    pub previous_block: Option<RatsBlock>,
    pub reuse_identical: bool,
    pub erase_fill: u8,
}

#[derive(Debug)]
pub enum ExAnimationSlotOptionIoError {
    Load(PayloadLoadError),
    MissingOwnership,
    Codec(ExAnimationSlotOptionError),
    Save(PayloadSaveError),
}

impl fmt::Display for ExAnimationSlotOptionIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ExAnimation slot-option I/O failed: {self:?}")
    }
}

impl std::error::Error for ExAnimationSlotOptionIoError {}

impl From<PayloadLoadError> for ExAnimationSlotOptionIoError {
    fn from(value: PayloadLoadError) -> Self {
        Self::Load(value)
    }
}

impl From<ExAnimationSlotOptionError> for ExAnimationSlotOptionIoError {
    fn from(value: ExAnimationSlotOptionError) -> Self {
        Self::Codec(value)
    }
}

impl From<PayloadSaveError> for ExAnimationSlotOptionIoError {
    fn from(value: PayloadSaveError) -> Self {
        Self::Save(value)
    }
}

impl Project {
    /// Loads Lunar Magic's RATS-owned seven-byte `ExAnimation` slot-option table.
    ///
    /// # Errors
    ///
    /// Rejects invalid pointer mapping, missing ownership, or a malformed payload length.
    pub fn load_exanimation_slot_options(
        &self,
        layout: ExAnimationSlotOptionRomLayout,
    ) -> Result<LoadedExAnimationSlotOptions, ExAnimationSlotOptionIoError> {
        let payload = self.load_tagged_payload(layout.pointer_offset, layout.mapper)?;
        let block = payload
            .block
            .ok_or(ExAnimationSlotOptionIoError::MissingOwnership)?;
        Ok(LoadedExAnimationSlotOptions {
            table: ExAnimationSlotOptionTable::decode(&payload.bytes)?,
            block,
        })
    }

    /// Transactionally relocates the exact seven-byte slot-option table.
    ///
    /// # Errors
    ///
    /// Rejects invalid option state, allocation or mapping failures, and unsafe pointer writes.
    pub fn save_exanimation_slot_options(
        &mut self,
        table: &ExAnimationSlotOptionTable,
        layout: ExAnimationSlotOptionRomLayout,
        options: &ExAnimationSlotOptionSaveOptions,
    ) -> Result<PayloadSaveResult, ExAnimationSlotOptionIoError> {
        Ok(self.save_tagged_payload(&PayloadSaveRequest {
            description: "save ExAnimation slot options".into(),
            payload: table.encode()?.to_vec(),
            pointer: PayloadPointer::contiguous(layout.pointer_offset),
            mapper: layout.mapper,
            allocation_policy: options.allocation.clone(),
            previous_block: options.previous_block.clone(),
            reuse_identical: options.reuse_identical,
            maximum_payload_len: EXANIMATION_LEVEL_SLOT_COUNT,
            erase_fill: options.erase_fill,
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::ExAnimationSlotOptions;
    use lm_rats::ProtectedRange;
    use lm_rom::RomImage;

    fn layout() -> ExAnimationSlotOptionRomLayout {
        ExAnimationSlotOptionRomLayout {
            mapper: Mapper::LoRom,
            pointer_offset: 0x20,
        }
    }

    fn table() -> ExAnimationSlotOptionTable {
        ExAnimationSlotOptionTable {
            slots: std::array::from_fn(|slot| ExAnimationSlotOptions {
                preserved_low_nibble: u8::try_from(slot).unwrap(),
                enabled: [slot % 2 == 0, slot % 3 == 0, slot % 4 == 0, slot % 5 == 0],
            }),
        }
    }

    fn options() -> ExAnimationSlotOptionSaveOptions {
        ExAnimationSlotOptionSaveOptions {
            allocation: AllocationPolicy {
                search: 0x100..0x8000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x20..0x23)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        }
    }

    #[test]
    fn save_load_and_undo_are_transactional() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let saved = project
            .save_exanimation_slot_options(&table(), layout(), &options())
            .unwrap();
        let loaded = project.load_exanimation_slot_options(layout()).unwrap();
        assert_eq!(loaded.table, table());
        assert_eq!(loaded.block, saved.block);
        assert!(project.history.undo(&mut project.rom).unwrap());
        assert_eq!(project.save_snapshot(), original);
    }

    #[test]
    fn invalid_low_nibble_does_not_mutate() {
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let original = project.save_snapshot();
        let mut invalid = table();
        invalid.slots[3].preserved_low_nibble = 0x80;
        assert!(matches!(
            project.save_exanimation_slot_options(&invalid, layout(), &options()),
            Err(ExAnimationSlotOptionIoError::Codec(
                ExAnimationSlotOptionError::LowNibbleOutOfRange { slot: 3, .. }
            ))
        ));
        assert_eq!(project.save_snapshot(), original);
    }
}
