//! Lunar Magic native overworld boss-sequence row storage.

use crate::{
    PatchFixup, PatchFixupEncoding, PatchPayload, PatchWrite, Project, RelocatablePatchError,
    RelocatablePatchPlan,
};
use lm_overworld::{BossSequenceMessage, BossSequenceMessageTable, BossSequenceTableError};
use lm_rats::{AllocationPolicy, RatsBlock, parse_at};
use lm_rom::{Mapper, RomError, snes_to_pc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BossSequencePatchLocator {
    pub mapper: Mapper,
    pub first_pointer: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BossSequenceStorage {
    LegacyRows,
    Combined(RatsBlock),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedBossSequenceMessages {
    pub table: BossSequenceMessageTable,
    pub storage: BossSequenceStorage,
}

#[derive(Debug)]
pub enum BossSequencePatchError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    Rom(RomError),
    Pointer { row: usize, source: RomError },
    InvalidCombinedPointer { row: usize },
    CombinedLength(usize),
    PointerOffsetOverflow { row: usize },
    Table(BossSequenceTableError),
    Install(RelocatablePatchError),
    ReopenMismatch,
}

impl std::fmt::Display for BossSequencePatchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "native boss-sequence patch failed: {self:?}")
    }
}

impl std::error::Error for BossSequencePatchError {}

impl From<RomError> for BossSequencePatchError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<BossSequenceTableError> for BossSequencePatchError {
    fn from(value: BossSequenceTableError) -> Self {
        Self::Table(value)
    }
}

impl From<RelocatablePatchError> for BossSequencePatchError {
    fn from(value: RelocatablePatchError) -> Self {
        Self::Install(value)
    }
}

impl Project {
    /// Loads the 56 legacy row pointers or one exact combined Lunar Magic allocation.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, invalid pointers, malformed combined ownership, ROM bounds,
    /// or noncanonical combined row records.
    pub fn load_boss_sequence_messages_detected(
        &self,
        locator: BossSequencePatchLocator,
    ) -> Result<LoadedBossSequenceMessages, BossSequencePatchError> {
        validate_mapper(self, locator.mapper)?;
        let pointers = read_pointers(self, locator)?;
        if let Some(block) = combined_block(self, &pointers)? {
            let payload = self.rom.read(block.payload.start, block.payload.len())?;
            return Ok(LoadedBossSequenceMessages {
                table: BossSequenceMessageTable::decode_native_payload(payload)?,
                storage: BossSequenceStorage::Combined(block),
            });
        }
        Ok(LoadedBossSequenceMessages {
            table: decode_legacy_rows(self, &pointers)?,
            storage: BossSequenceStorage::LegacyRows,
        })
    }

    /// Installs or replaces all 56 rows in one RATS allocation and republishes every pointer.
    ///
    /// # Errors
    ///
    /// Rejects malformed current pointers/ownership, allocation or checksum failures, stale
    /// pointer preconditions, and semantic disagreement after reopen. Failure is atomic.
    pub fn save_boss_sequence_messages_detected(
        &mut self,
        table: &BossSequenceMessageTable,
        locator: BossSequencePatchLocator,
        allocation: &AllocationPolicy,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, BossSequencePatchError> {
        let loaded = self.load_boss_sequence_messages_detected(locator)?;
        if loaded.table == *table {
            return Ok(false);
        }
        let payload = table.encode_native_payload();
        let mut writes = Vec::with_capacity(BossSequenceMessageTable::ROW_COUNT);
        for row in 0..BossSequenceMessageTable::ROW_COUNT {
            let offset = pointer_offset(locator, row)?;
            writes.push(PatchWrite {
                offset,
                expected: self.rom.read(offset, 3)?.to_vec(),
                replacement: vec![0; 3],
                fixups: vec![PatchFixup {
                    offset: 0,
                    target_payload: 0,
                    target_addend: row * BossSequenceMessageTable::NATIVE_ROW_LEN,
                    encoding: PatchFixupEncoding::Long24,
                }],
            });
        }
        self.install_relocatable_patch(&RelocatablePatchPlan {
            description: "save native overworld boss-sequence messages".into(),
            mapper: locator.mapper,
            allocation: allocation.clone(),
            checksum_field,
            expansion_fill: fill,
            payloads: vec![PatchPayload {
                bytes: payload,
                fixups: Vec::new(),
            }],
            writes,
        })?;
        if self.load_boss_sequence_messages_detected(locator)?.table != *table {
            return Err(BossSequencePatchError::ReopenMismatch);
        }
        Ok(true)
    }
}

fn validate_mapper(project: &Project, mapper: Mapper) -> Result<(), BossSequencePatchError> {
    if let Some(identity) = &project.identity
        && identity.mapper != mapper
    {
        return Err(BossSequencePatchError::MapperMismatch {
            expected: identity.mapper,
            actual: mapper,
        });
    }
    Ok(())
}

fn pointer_offset(
    locator: BossSequencePatchLocator,
    row: usize,
) -> Result<usize, BossSequencePatchError> {
    locator
        .first_pointer
        .checked_add(
            row.checked_mul(3)
                .ok_or(BossSequencePatchError::PointerOffsetOverflow { row })?,
        )
        .ok_or(BossSequencePatchError::PointerOffsetOverflow { row })
}

fn read_pointers(
    project: &Project,
    locator: BossSequencePatchLocator,
) -> Result<[usize; BossSequenceMessageTable::ROW_COUNT], BossSequencePatchError> {
    let mut pointers = [0; BossSequenceMessageTable::ROW_COUNT];
    for (row, pointer) in pointers.iter_mut().enumerate() {
        let offset = pointer_offset(locator, row)?;
        let bytes = project.rom.read(offset, 3)?;
        let address = u32::from(bytes[0]) | u32::from(bytes[1]) << 8 | u32::from(bytes[2]) << 16;
        *pointer = snes_to_pc(locator.mapper, address)
            .map_err(|source| BossSequencePatchError::Pointer { row, source })?;
    }
    Ok(pointers)
}

fn combined_block(
    project: &Project,
    pointers: &[usize; BossSequenceMessageTable::ROW_COUNT],
) -> Result<Option<RatsBlock>, BossSequencePatchError> {
    let Some(header) = pointers[0].checked_sub(lm_rats::HEADER_LEN) else {
        return Ok(None);
    };
    let Ok(block) = parse_at(project.rom.logical_bytes(), header) else {
        return Ok(None);
    };
    if block.payload.start != pointers[0] {
        return Ok(None);
    }
    if block.payload.len() != BossSequenceMessageTable::NATIVE_PAYLOAD_LEN {
        return Err(BossSequencePatchError::CombinedLength(block.payload.len()));
    }
    for (row, pointer) in pointers.iter().enumerate() {
        let expected = block.payload.start + row * BossSequenceMessageTable::NATIVE_ROW_LEN;
        if *pointer != expected {
            return Err(BossSequencePatchError::InvalidCombinedPointer { row });
        }
    }
    Ok(Some(block))
}

fn decode_legacy_rows(
    project: &Project,
    pointers: &[usize; BossSequenceMessageTable::ROW_COUNT],
) -> Result<BossSequenceMessageTable, BossSequencePatchError> {
    let mut glyphs =
        vec![BossSequenceMessageTable::BLANK_GLYPH; BossSequenceMessageTable::MESSAGE_COUNT * 192];
    for (row, pointer) in pointers.iter().copied().enumerate() {
        let header = project.rom.read(pointer, 4)?;
        let length = if (1..0x80).contains(&header[0]) {
            ((((usize::from(header[2]) & 0x3f) << 8) | usize::from(header[3])) + 1)
                .min(BossSequenceMessageTable::INTERLEAVED_ROW_LEN)
        } else {
            0
        };
        let mut interleaved = [0; BossSequenceMessageTable::INTERLEAVED_ROW_LEN];
        for (index, byte) in interleaved.iter_mut().enumerate() {
            *byte = if index & 1 == 0 {
                BossSequenceMessageTable::BLANK_GLYPH
            } else {
                BossSequenceMessageTable::ATTRIBUTE_BYTE
            };
        }
        interleaved[..length].copy_from_slice(project.rom.read(pointer + 4, length)?);
        let destination = row * BossSequenceMessageTable::ROW_GLYPHS;
        for (column, glyph) in interleaved.iter().step_by(2).copied().enumerate() {
            glyphs[destination + column] = glyph;
        }
    }
    let mut rows = glyphs.chunks_exact(BossSequenceMessage::ENCODED_LEN);
    Ok(BossSequenceMessageTable {
        messages: std::array::from_fn(|_| {
            BossSequenceMessage::decode(rows.next().unwrap_or(&[])).unwrap_or(BossSequenceMessage(
                [BossSequenceMessageTable::BLANK_GLYPH; 192],
            ))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_rom::{RomImage, pc_to_snes};

    const LOCATOR: BossSequencePatchLocator = BossSequencePatchLocator {
        mapper: Mapper::LoRom,
        first_pointer: 0x100,
    };

    #[test]
    fn legacy_rows_decode_and_combined_rows_are_detected_exactly() {
        let mut bytes = vec![0xff; 0x10_000];
        for row in 0..BossSequenceMessageTable::ROW_COUNT {
            let payload = 0x1000 + row * 0x40;
            bytes[payload..payload + 4].copy_from_slice(&[0x53, 0x44, 0, 0x2f]);
            for column in 0..BossSequenceMessageTable::ROW_GLYPHS {
                bytes[payload + 4 + column * 2] = row.to_le_bytes()[0];
                bytes[payload + 5 + column * 2] = 0x39;
            }
            let pointer = pc_to_snes(Mapper::LoRom, payload).unwrap().to_le_bytes();
            let offset = LOCATOR.first_pointer + row * 3;
            bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
        }
        let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
        let mut loaded = project
            .load_boss_sequence_messages_detected(LOCATOR)
            .unwrap();
        assert!(matches!(loaded.storage, BossSequenceStorage::LegacyRows));
        assert_eq!(loaded.table.messages[1].0[0], 8);
        loaded.table.messages[6].clone_from(&BossSequenceMessage([0xab; 192]));
        let policy = AllocationPolicy::lorom(0x4000..0x10_000);
        project
            .save_boss_sequence_messages_detected(&loaded.table, LOCATOR, &policy, 0x7fdc, 0xff)
            .unwrap();
        let reopened = project
            .load_boss_sequence_messages_detected(LOCATOR)
            .unwrap();
        assert_eq!(reopened.table, loaded.table);
        assert!(matches!(reopened.storage, BossSequenceStorage::Combined(_)));
    }
}
