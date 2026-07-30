//! Installed Lunar Magic expanded per-level mode/settings table I/O.

use crate::{Project, RomWrite, TransactionError};
use lm_rom::{Mapper, RomError, compute_snes_checksum, snes_to_pc};

/// Stable hooks which publish the allocator-dependent expanded level-mode table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpandedLevelModeLocator {
    pub mapper: Mapper,
    pub hook_offsets: [usize; 2],
    pub runtime_to_table_bias: usize,
    pub entries: usize,
}

#[derive(Debug)]
pub enum ExpandedLevelModeIoError {
    SlotOutOfRange { slot: usize, entries: usize },
    MissingHook { offset: usize, actual: Vec<u8> },
    HookTargetsDisagree { first: usize, second: usize },
    TableBiasUnderflow { runtime: usize, bias: usize },
    OffsetOverflow,
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for ExpandedLevelModeIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "expanded level-mode I/O failed: {self:?}")
    }
}

impl std::error::Error for ExpandedLevelModeIoError {}

impl From<RomError> for ExpandedLevelModeIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for ExpandedLevelModeIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl ExpandedLevelModeLocator {
    /// Resolves and cross-checks the table published by both runtime hooks.
    ///
    /// # Errors
    ///
    /// Rejects missing JSL hooks, invalid SNES pointers, inconsistent targets, or bias underflow.
    pub fn resolve(self, project: &Project) -> Result<usize, ExpandedLevelModeIoError> {
        let first = self.resolve_hook(project, self.hook_offsets[0])?;
        let second = self.resolve_hook(project, self.hook_offsets[1])?;
        if first != second {
            return Err(ExpandedLevelModeIoError::HookTargetsDisagree { first, second });
        }
        first.checked_sub(self.runtime_to_table_bias).ok_or(
            ExpandedLevelModeIoError::TableBiasUnderflow {
                runtime: first,
                bias: self.runtime_to_table_bias,
            },
        )
    }

    fn resolve_hook(
        self,
        project: &Project,
        offset: usize,
    ) -> Result<usize, ExpandedLevelModeIoError> {
        let hook = project.rom.read(offset, 4)?;
        if hook[0] != 0x22 {
            return Err(ExpandedLevelModeIoError::MissingHook {
                offset,
                actual: hook.to_vec(),
            });
        }
        let address = u32::from(hook[1]) | u32::from(hook[2]) << 8 | u32::from(hook[3]) << 16;
        Ok(snes_to_pc(self.mapper, address)?)
    }
}

impl Project {
    /// Loads one installed expanded level-mode/settings byte.
    ///
    /// # Errors
    ///
    /// Rejects an invalid slot or locator and ROM bounds failures.
    pub fn load_expanded_level_mode(
        &self,
        slot: usize,
        locator: ExpandedLevelModeLocator,
    ) -> Result<u8, ExpandedLevelModeIoError> {
        validate_slot(slot, locator.entries)?;
        let table = locator.resolve(self)?;
        let offset = table
            .checked_add(slot)
            .ok_or(ExpandedLevelModeIoError::OffsetOverflow)?;
        Ok(self.rom.read(offset, 1)?[0])
    }

    /// Saves the persistent low seven bits in one checksum-valid undoable transaction.
    ///
    /// Lunar Magic recomputes bit 7 from the selected level layout before persistence. Controlled
    /// Wine imports prove that an MWL containing only bit 7 produces no ROM change.
    ///
    /// # Errors
    ///
    /// Rejects an invalid slot or locator, ROM bounds/checksum failures, or transaction failures.
    pub fn save_expanded_level_mode(
        &mut self,
        slot: usize,
        value: u8,
        locator: ExpandedLevelModeLocator,
        checksum_field: usize,
    ) -> Result<bool, ExpandedLevelModeIoError> {
        validate_slot(slot, locator.entries)?;
        let table = locator.resolve(self)?;
        let offset = table
            .checked_add(slot)
            .ok_or(ExpandedLevelModeIoError::OffsetOverflow)?;
        let write = RomWrite {
            offset,
            bytes: vec![value & 0x7f],
        };
        if !self.writes_would_change(std::slice::from_ref(&write))? {
            return Ok(false);
        }
        let mut staged = self.rom.clone();
        staged.write(write.offset, &write.bytes)?;
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        Ok(self.apply_writes(
            "save expanded level mode",
            &[
                write,
                RomWrite {
                    offset: checksum_field,
                    bytes: checksum.encoded().to_vec(),
                },
            ],
        )?)
    }
}

fn validate_slot(slot: usize, entries: usize) -> Result<(), ExpandedLevelModeIoError> {
    if slot >= entries {
        return Err(ExpandedLevelModeIoError::SlotOutOfRange { slot, entries });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{RomImage, pc_to_snes};

    fn fixture() -> (Project, ExpandedLevelModeLocator) {
        let mut bytes = vec![0; 0x10000];
        let runtime = 0x9000;
        let pointer = pc_to_snes(Mapper::LoRom, runtime).unwrap().to_le_bytes();
        for hook in [0x100, 0x200] {
            bytes[hook..hook + 4].copy_from_slice(&[0x22, pointer[0], pointer[1], pointer[2]]);
        }
        let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
        bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
        (
            Project::new(RomImage::from_bytes(bytes).unwrap()),
            ExpandedLevelModeLocator {
                mapper: Mapper::LoRom,
                hook_offsets: [0x100, 0x200],
                runtime_to_table_bias: 0x240,
                entries: 0x200,
            },
        )
    }

    #[test]
    fn resolves_both_hooks_and_saves_persistent_bits_atomically() {
        let (mut project, locator) = fixture();
        assert!(
            project
                .save_expanded_level_mode(5, 0xc3, locator, 0x7fdc)
                .unwrap()
        );
        assert_eq!(project.load_expanded_level_mode(5, locator).unwrap(), 0x43);
        assert!(project.undo().unwrap());
        assert_eq!(project.load_expanded_level_mode(5, locator).unwrap(), 0);
    }

    #[test]
    fn disagreement_is_rejected_without_history() {
        let (mut project, locator) = fixture();
        project.rom.write(0x201, &[0x00, 0x80, 0x81]).unwrap();
        assert!(matches!(
            project.save_expanded_level_mode(0, 3, locator, 0x7fdc),
            Err(ExpandedLevelModeIoError::HookTargetsDisagree { .. })
        ));
        assert_eq!(project.history.undo_len(), 0);
    }
}
