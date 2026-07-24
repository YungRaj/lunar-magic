use crate::Edit;
use lm_rom::{Mapper, RomError, RomImage};
use std::fmt;

pub struct RomTransaction<'a> {
    rom: &'a mut RomImage,
    edits: Vec<Edit>,
    committed: bool,
}

impl<'a> RomTransaction<'a> {
    #[must_use]
    pub fn new(rom: &'a mut RomImage) -> Self {
        Self {
            rom,
            edits: Vec::new(),
            committed: false,
        }
    }

    /// Stages and immediately applies one reversible write.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] for out-of-range writes.
    pub fn write(
        &mut self,
        offset: usize,
        bytes: &[u8],
        description: impl Into<String>,
    ) -> Result<(), TransactionError> {
        let before = self.rom.read(offset, bytes.len())?.to_vec();
        if before == bytes {
            return Ok(());
        }
        self.rom.write(offset, bytes)?;
        self.edits.push(Edit {
            offset,
            before,
            after: bytes.to_vec(),
            description: description.into(),
        });
        Ok(())
    }

    /// Appends a logical ROM tail as a reversible history edit.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] if the image changed unexpectedly or the resulting image is
    /// invalid.
    pub fn append(
        &mut self,
        bytes: &[u8],
        description: impl Into<String>,
    ) -> Result<(), TransactionError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let offset = self.rom.logical_len();
        self.rom.replace_logical_tail(offset, &[], bytes)?;
        self.edits.push(Edit {
            offset,
            before: Vec::new(),
            after: bytes.to_vec(),
            description: description.into(),
        });
        Ok(())
    }

    #[must_use]
    pub fn commit(mut self) -> Vec<Edit> {
        self.committed = true;
        std::mem::take(&mut self.edits)
    }
}

impl Drop for RomTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            for edit in self.edits.iter().rev() {
                let _ = edit.revert(self.rom);
            }
        }
    }
}

#[derive(Debug)]
pub enum TransactionError {
    Rom(RomError),
    WriteRangeOverflow {
        index: usize,
    },
    OverlappingWrites {
        first: usize,
        second: usize,
    },
    UnexpectedLogicalLength {
        expected: usize,
        actual: usize,
    },
    MutationLengthOverflow,
    MutationMapperMismatch {
        expected: Mapper,
        actual: Mapper,
    },
    MutationMapperCannotAddressImage {
        mapper: Mapper,
        image_len: usize,
    },
    InvalidMutationExpansionSize(usize),
    CannotPrepareShrink {
        before: usize,
        after: usize,
    },
    WriteOutsideMutation {
        index: usize,
        offset: usize,
        len: usize,
        image_len: usize,
    },
}

impl From<RomError> for TransactionError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "transaction failed: {self:?}")
    }
}

impl std::error::Error for TransactionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_on_drop() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        {
            let mut tx = RomTransaction::new(&mut rom);
            tx.write(1, &[7], "test").unwrap();
        }
        assert_eq!(rom.read(1, 1).unwrap(), &[0]);
    }

    #[test]
    fn committed_write_survives() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let edits = {
            let mut tx = RomTransaction::new(&mut rom);
            tx.write(1, &[7], "test").unwrap();
            tx.commit()
        };
        assert_eq!(rom.read(1, 1).unwrap(), &[7]);
        assert_eq!(edits.len(), 1);
    }

    #[test]
    fn byte_identical_write_does_not_create_an_edit() {
        let mut bytes = vec![0; 0x8000];
        bytes[4..6].copy_from_slice(&[1, 2]);
        let mut rom = RomImage::from_bytes(bytes).unwrap();
        let edits = {
            let mut transaction = RomTransaction::new(&mut rom);
            transaction.write(4, &[1, 2], "no-op").unwrap();
            transaction.commit()
        };
        assert!(edits.is_empty());
        assert!(rom.changed_ranges().is_empty());
    }

    #[test]
    fn appended_tail_rolls_back_and_commits_reversibly() {
        let mut rom = RomImage::from_bytes(vec![0; 0x8000]).unwrap();
        let tail = vec![0xff; 0x8000];
        {
            let mut transaction = RomTransaction::new(&mut rom);
            transaction.append(&tail, "expand").unwrap();
        }
        assert_eq!(rom.logical_len(), 0x8000);

        let edit = {
            let mut transaction = RomTransaction::new(&mut rom);
            transaction.append(&tail, "expand").unwrap();
            transaction.commit().remove(0)
        };
        assert_eq!(rom.logical_len(), 0x10000);
        edit.revert(&mut rom).unwrap();
        assert_eq!(rom.logical_len(), 0x8000);
        edit.apply(&mut rom).unwrap();
        assert_eq!(rom.logical_len(), 0x10000);
    }
}
