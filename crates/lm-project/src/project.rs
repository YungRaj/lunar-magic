use crate::{CopierHeaderEdit, EditBatch, EditKind, History, RomTransaction, TransactionError};
use lm_level::Level;
use lm_overworld::Overworld;
use lm_rom::{
    COPIER_HEADER_LEN, CopierHeader, IdentityError, RomIdentity, RomImage, SnesChecksum,
    compute_snes_checksum, detect_identity, mapper_supports_image_len,
};
use std::collections::BTreeMap;

mod mutation;

use mutation::validate_write_shapes;
pub use mutation::{RomMutation, RomWrite};

#[derive(Clone, Debug)]
pub struct Project {
    pub rom: RomImage,
    pub identity: Option<RomIdentity>,
    pub levels: BTreeMap<u16, Level>,
    pub overworld: Overworld,
    pub history: History,
}

impl Project {
    #[must_use]
    pub fn new(rom: RomImage) -> Self {
        Self::with_history_limit(rom, 100)
    }

    /// Creates a project with an explicit bounded undo-operation count.
    #[must_use]
    pub fn with_history_limit(rom: RomImage, history_limit: usize) -> Self {
        Self {
            rom,
            identity: None,
            levels: BTreeMap::new(),
            overworld: Overworld::default(),
            history: History::with_limit(history_limit),
        }
    }

    /// Opens a ROM accepted by the recovered Lunar Magic identity rules.
    ///
    /// # Errors
    ///
    /// Returns [`IdentityError`] for unsupported games, regions, revisions, or headers.
    pub fn open_supported(rom: RomImage) -> Result<Self, IdentityError> {
        let identity = detect_identity(&rom)?;
        let mut project = Self::new(rom);
        project.identity = Some(identity);
        Ok(project)
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.rom.has_file_changes()
    }

    /// Refreshes the stored and computed checksum evidence in the cached cartridge identity.
    ///
    /// Returns `false` when this project was not identity-qualified or the current image no longer
    /// contains the cached internal-header checksum field. Stable game, region, revision, and
    /// mapper fields are deliberately not redetected by this narrow synchronization operation.
    pub fn synchronize_identity_checksums(&mut self) -> bool {
        let Some(identity) = self.identity.as_mut() else {
            return false;
        };
        let Some(field) = identity.internal_header_offset.checked_add(0x1c) else {
            return false;
        };
        let Ok(stored) = SnesChecksum::decode(self.rom.logical_bytes(), field) else {
            return false;
        };
        let Ok(computed) = compute_snes_checksum(self.rom.logical_bytes(), field) else {
            return false;
        };
        identity.stored_checksum = stored;
        identity.computed_checksum = computed;
        true
    }

    /// Undoes one project operation and synchronizes cached checksum evidence after success.
    ///
    /// # Errors
    ///
    /// Returns a ROM error while preserving retryable history state when an edit cannot revert.
    pub fn undo(&mut self) -> Result<bool, lm_rom::RomError> {
        let changed = self.history.undo(&mut self.rom)?;
        if changed {
            self.synchronize_identity_checksums();
        }
        Ok(changed)
    }

    /// Redoes one project operation and synchronizes cached checksum evidence after success.
    ///
    /// # Errors
    ///
    /// Returns a ROM error while preserving retryable history state when an edit cannot apply.
    pub fn redo(&mut self) -> Result<bool, lm_rom::RomError> {
        let changed = self.history.redo(&mut self.rom)?;
        if changed {
            self.synchronize_identity_checksums();
        }
        Ok(changed)
    }

    /// Applies a group of ROM writes as one atomic, undoable user operation.
    ///
    /// All ranges are validated before mutation. If any write fails, every earlier write in the
    /// group is reverted by the transaction guard.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] when any write is outside the logical ROM image.
    pub fn apply_writes(
        &mut self,
        description: impl Into<String>,
        writes: &[RomWrite],
    ) -> Result<bool, TransactionError> {
        let description = description.into();
        if !self.writes_would_change(writes)? {
            return Ok(false);
        }
        let mut transaction = RomTransaction::new(&mut self.rom);
        for write in writes {
            transaction.write(write.offset, &write.bytes, description.clone())?;
        }
        let edits = transaction.commit();
        if edits.is_empty() {
            return Ok(false);
        }
        self.history.push_batch(EditBatch {
            description,
            edits,
            kind: EditKind::Ordinary,
            copier_header: None,
        });
        self.synchronize_identity_checksums();
        Ok(true)
    }

    /// Adds or removes the physical 512-byte copier header as one reversible operation.
    ///
    /// Logical ROM bytes and cached cartridge identity remain unchanged. A newly added header is
    /// initialized entirely with `fill`; removing a header retains its exact bytes in history.
    ///
    /// # Errors
    ///
    /// Returns a ROM mismatch error if the current physical header changes before the guarded
    /// history edit is applied.
    pub fn set_copier_header(
        &mut self,
        description: impl Into<String>,
        target: CopierHeader,
        fill: u8,
    ) -> Result<bool, lm_rom::RomError> {
        if self.rom.copier_header() == target {
            return Ok(false);
        }
        let before = self.rom.copier_header_bytes().map(<[u8]>::to_vec);
        let after = match target {
            CopierHeader::Absent => None,
            CopierHeader::Present => Some(vec![fill; COPIER_HEADER_LEN]),
        };
        let header = CopierHeaderEdit { before, after };
        header.apply(&mut self.rom)?;
        self.history.push_batch(EditBatch {
            description: description.into(),
            edits: Vec::new(),
            kind: EditKind::Ordinary,
            copier_header: Some(header),
        });
        Ok(true)
    }

    /// Compare-replaces the complete physical copier header as one reversible operation.
    ///
    /// Logical ROM bytes and cached cartridge identity remain unchanged. Unlike
    /// [`Self::set_copier_header`], this retains the caller's complete structured header rather
    /// than synthesizing a uniform fill.
    ///
    /// # Errors
    ///
    /// Returns a ROM mismatch error when `replacement` is not exactly 512 bytes.
    pub fn set_copier_header_exact(
        &mut self,
        description: impl Into<String>,
        replacement: &[u8],
    ) -> Result<bool, lm_rom::RomError> {
        if self.rom.copier_header_bytes() == Some(replacement) {
            return Ok(false);
        }
        if replacement.len() != COPIER_HEADER_LEN {
            return Err(lm_rom::RomError::RangeOutOfBounds {
                offset: 0,
                len: replacement.len(),
                image_len: COPIER_HEADER_LEN,
            });
        }
        let header = CopierHeaderEdit {
            before: self.rom.copier_header_bytes().map(<[u8]>::to_vec),
            after: Some(replacement.to_vec()),
        };
        header.apply(&mut self.rom)?;
        self.history.push_batch(EditBatch {
            description: description.into(),
            edits: Vec::new(),
            kind: EditKind::Ordinary,
            copier_header: Some(header),
        });
        Ok(true)
    }

    /// Applies a prepared append-plus-write mutation as one atomic undoable operation.
    ///
    /// The expected logical length prevents a mutation prepared from a differently sized image
    /// from being replayed. Append happens inside the same transaction as all disjoint writes, so
    /// any later range failure rolls back the new tail as well.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] for a stale length, overflow, overlap, or any write outside the
    /// resulting logical image.
    pub fn apply_mutation(
        &mut self,
        description: impl Into<String>,
        mutation: &RomMutation,
    ) -> Result<bool, TransactionError> {
        self.apply_mutation_with_kind(description, mutation, EditKind::Ordinary)
    }

    /// Replaces one complete logical image as a single reversible tail edit.
    ///
    /// The longest common prefix is retained in place and the remaining tail is compare-replaced,
    /// allowing bounded import formats such as IPS to grow or shrink a mapper-valid ROM without
    /// bypassing project history. The optional copier header is outside this logical boundary and
    /// remains byte-exact.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] when the target is not a complete mapper-addressable bank or
    /// when the current project's qualified mapper differs from `mapper`.
    pub fn apply_logical_replacement(
        &mut self,
        description: impl Into<String>,
        mapper: lm_rom::Mapper,
        target: &[u8],
    ) -> Result<bool, TransactionError> {
        if let Some(identity) = &self.identity
            && identity.mapper != mapper
        {
            return Err(TransactionError::MutationMapperMismatch {
                expected: identity.mapper,
                actual: mapper,
            });
        }
        if target.len() % 0x8000 != 0 || !mapper_supports_image_len(mapper, target.len()) {
            return Err(TransactionError::InvalidMutationExpansionSize(target.len()));
        }
        let before = self.rom.logical_bytes();
        if before == target {
            return Ok(false);
        }
        let common = before
            .iter()
            .zip(target)
            .take_while(|(left, right)| left == right)
            .count();
        let edit = crate::Edit {
            offset: common,
            before: before[common..].to_vec(),
            after: target[common..].to_vec(),
            description: description.into(),
        };
        edit.apply(&mut self.rom)?;
        self.history.push(edit);
        self.synchronize_identity_checksums();
        Ok(true)
    }

    pub(crate) fn apply_mutation_with_kind(
        &mut self,
        description: impl Into<String>,
        mutation: &RomMutation,
        kind: EditKind,
    ) -> Result<bool, TransactionError> {
        let description = description.into();
        if !self.mutation_would_change(mutation)? {
            return Ok(false);
        }
        let mut transaction = RomTransaction::new(&mut self.rom);
        transaction.append(&mutation.appended, description.clone())?;
        for write in &mutation.writes {
            transaction.write(write.offset, &write.bytes, description.clone())?;
        }
        let edits = transaction.commit();
        if edits.is_empty() {
            return Ok(false);
        }
        self.history.push_batch(EditBatch {
            description,
            edits,
            kind,
            copier_header: None,
        });
        self.synchronize_identity_checksums();
        Ok(true)
    }

    /// Validates a prepared mutation and reports whether it changes the current image.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] without mutation for stale length, arithmetic/range errors, or
    /// overlapping writes.
    pub fn mutation_would_change(&self, mutation: &RomMutation) -> Result<bool, TransactionError> {
        let actual = self.rom.logical_len();
        if mutation.expected_len != actual {
            return Err(TransactionError::UnexpectedLogicalLength {
                expected: mutation.expected_len,
                actual,
            });
        }
        let target_len = actual
            .checked_add(mutation.appended.len())
            .ok_or(TransactionError::MutationLengthOverflow)?;
        if let Some(identity) = &self.identity
            && identity.mapper != mutation.mapper
        {
            return Err(TransactionError::MutationMapperMismatch {
                expected: identity.mapper,
                actual: mutation.mapper,
            });
        }
        let mapper_cannot_address = !mapper_supports_image_len(mutation.mapper, target_len);
        if mutation.appended.is_empty() && mapper_cannot_address {
            return Err(TransactionError::MutationMapperCannotAddressImage {
                mapper: mutation.mapper,
                image_len: target_len,
            });
        }
        if !mutation.appended.is_empty() && (target_len % 0x8000 != 0 || mapper_cannot_address) {
            return Err(TransactionError::InvalidMutationExpansionSize(target_len));
        }
        validate_write_shapes(&mutation.writes, target_len)?;
        if !mutation.appended.is_empty() {
            return Ok(true);
        }
        self.writes_would_change(&mutation.writes)
    }

    /// Validates a write batch and reports whether it differs from current ROM bytes.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] if any range falls outside the logical ROM image.
    pub fn writes_would_change(&self, writes: &[RomWrite]) -> Result<bool, TransactionError> {
        let mut changed = false;
        for (index, write) in writes.iter().enumerate() {
            changed |= self.rom.read(write.offset, write.bytes.len())? != write.bytes;
            if write.bytes.is_empty() {
                continue;
            }
            let end = write
                .offset
                .checked_add(write.bytes.len())
                .ok_or(TransactionError::WriteRangeOverflow { index })?;
            for (other_index, other) in writes[..index].iter().enumerate() {
                let other_end = other
                    .offset
                    .checked_add(other.bytes.len())
                    .ok_or(TransactionError::WriteRangeOverflow { index: other_index })?;
                if !other.bytes.is_empty() && write.offset < other_end && other.offset < end {
                    return Err(TransactionError::OverlappingWrites {
                        first: other_index,
                        second: index,
                    });
                }
            }
        }
        Ok(changed)
    }

    /// Returns bytes for a save-as operation without changing dirty state.
    #[must_use]
    pub fn save_snapshot(&self) -> Vec<u8> {
        self.rom.as_file_bytes().to_vec()
    }

    /// Marks the current ROM state as successfully persisted.
    pub fn mark_saved(&mut self) {
        self.rom.accept_changes();
    }

    /// Recomputes the internal-header checksum as one undoable edit.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError`] if the checksum field is outside the ROM.
    pub fn refresh_checksum(
        &mut self,
        logical_field_offset: usize,
    ) -> Result<SnesChecksum, TransactionError> {
        let checksum = compute_snes_checksum(self.rom.logical_bytes(), logical_field_offset)?;
        self.apply_writes(
            "Update SNES checksum",
            &[RomWrite {
                offset: logical_field_offset,
                bytes: checksum.encoded().to_vec(),
            }],
        )?;
        Ok(checksum)
    }
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
