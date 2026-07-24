use crate::{Project, RomMutation, TransactionError};
use lm_rom::{Mapper, SnesChecksum};

impl Project {
    /// Expands the logical ROM and repairs its checksum as one undoable transaction.
    ///
    /// The copier header is outside logical coordinates and remains byte-exact. Expansion is
    /// staged on a private clone; mapper extent, bank alignment, checksum range, and mutation
    /// preparation must all succeed before this project or its history changes.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] for shrinking, invalid mapper extent/alignment, checksum range,
    /// mutation preparation, or commit failure. The receiver is unchanged on every error.
    pub fn expand_rom(
        &mut self,
        mapper: Mapper,
        target_logical_len: usize,
        fill: u8,
        checksum_field: usize,
    ) -> Result<Option<SnesChecksum>, TransactionError> {
        if target_logical_len == self.rom.logical_len() {
            return Ok(None);
        }
        let before = self.rom.logical_bytes().to_vec();
        let mut staged = self.rom.clone();
        staged.expand(mapper, target_logical_len, fill)?;
        let checksum = staged.update_snes_checksum(checksum_field)?;
        let mutation = RomMutation::between(mapper, &before, staged.logical_bytes())?;
        let changed = self.apply_mutation("Expand ROM", &mutation)?;
        debug_assert!(changed);
        Ok(Some(checksum))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{RomImage, compute_snes_checksum};

    #[test]
    fn headered_expansion_checksum_and_history_are_one_atomic_operation() {
        let header = vec![0x5a; 0x200];
        let mut bytes = header.clone();
        bytes.extend(vec![0x11; 0x8000]);
        let mut project = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());

        let checksum = project
            .expand_rom(Mapper::LoRom, 0x1_0000, 0xff, 0x7fdc)
            .unwrap()
            .unwrap();
        assert_eq!(&project.rom.as_file_bytes()[..0x200], header);
        assert_eq!(project.rom.logical_len(), 0x1_0000);
        assert!(
            project.rom.logical_bytes()[0x8000..]
                .iter()
                .enumerate()
                .all(|(index, byte)| (0x7fdc..0x7fe0).contains(&(index + 0x8000)) || *byte == 0xff)
        );
        assert_eq!(
            checksum,
            compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
        );
        assert_eq!(project.history.undo_len(), 1);

        project.history.undo(&mut project.rom).unwrap();
        assert_eq!(project.rom.as_file_bytes(), bytes);
        project.history.redo(&mut project.rom).unwrap();
        assert_eq!(project.rom.logical_len(), 0x1_0000);
    }

    #[test]
    fn no_op_and_late_failures_preserve_rom_and_history() {
        let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
        let before = project.rom.as_file_bytes().to_vec();
        assert!(
            project
                .expand_rom(Mapper::LoRom, 0x8000, 0xff, 0x7fdc)
                .unwrap()
                .is_none()
        );
        assert!(
            project
                .expand_rom(Mapper::LoRom, 0x1_0000, 0xff, usize::MAX)
                .is_err()
        );
        assert!(
            project
                .expand_rom(Mapper::LoRom, 0x8001, 0xff, 0x7fdc)
                .is_err()
        );
        assert_eq!(project.rom.as_file_bytes(), before);
        assert_eq!(project.history.undo_len(), 0);
    }
}
