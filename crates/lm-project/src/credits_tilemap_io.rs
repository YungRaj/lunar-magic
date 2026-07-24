//! Strict legacy credits tilemap loading and in-place persistence.

use crate::{Project, RomWrite, TransactionError};
use lm_overworld::{CreditsTilemap, CreditsTilemapError};
use lm_rom::{Mapper, RomError, compute_snes_checksum};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyCreditsTilemapLayout {
    pub mapper: Mapper,
    pub records: usize,
    pub offsets: usize,
    pub row_count: usize,
    pub blank_word: u16,
}

#[derive(Debug)]
pub enum CreditsTilemapIoError {
    MapperMismatch { expected: Mapper, actual: Mapper },
    InvalidRowCount(usize),
    InvalidLayout,
    NonblankExpandedRow(usize),
    EncodedRecordsTooLarge { actual: usize, capacity: usize },
    Tilemap(CreditsTilemapError),
    Rom(RomError),
    Transaction(TransactionError),
}

impl std::fmt::Display for CreditsTilemapIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "credits tilemap I/O failed: {self:?}")
    }
}

impl std::error::Error for CreditsTilemapIoError {}

impl From<CreditsTilemapError> for CreditsTilemapIoError {
    fn from(value: CreditsTilemapError) -> Self {
        Self::Tilemap(value)
    }
}

impl From<RomError> for CreditsTilemapIoError {
    fn from(value: RomError) -> Self {
        Self::Rom(value)
    }
}

impl From<TransactionError> for CreditsTilemapIoError {
    fn from(value: TransactionError) -> Self {
        Self::Transaction(value)
    }
}

impl Project {
    /// Loads Lunar Magic's fixed 202-row legacy credits representation.
    ///
    /// # Errors
    ///
    /// Rejects mapper disagreement, malformed layout ranges, offsets, or row records.
    pub fn load_legacy_credits_tilemap(
        &self,
        layout: LegacyCreditsTilemapLayout,
    ) -> Result<CreditsTilemap, CreditsTilemapIoError> {
        validate(self, layout)?;
        let record_len = layout
            .offsets
            .checked_sub(layout.records)
            .ok_or(CreditsTilemapIoError::InvalidLayout)?;
        let records = self.rom.read(layout.records, record_len)?;
        let offset_bytes = self.rom.read(
            layout.offsets,
            layout
                .row_count
                .checked_mul(2)
                .ok_or(CreditsTilemapIoError::InvalidLayout)?,
        )?;
        let offsets: Vec<_> = offset_bytes
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect();
        Ok(CreditsTilemap::decode_rows(
            &offsets,
            records,
            layout.blank_word,
        )?)
    }

    /// Re-encodes legacy rows in place, clears unused record capacity, and repairs the checksum.
    ///
    /// The 54 expanded-only rows must remain blank. Larger edits use the separate expanded
    /// credits installer and are deliberately rejected here instead of being truncated.
    ///
    /// # Errors
    ///
    /// Rejects lossy expanded rows, record-capacity overflow, malformed layouts, bounds, checksum,
    /// or transaction failures atomically.
    pub fn save_legacy_credits_tilemap(
        &mut self,
        tilemap: &CreditsTilemap,
        layout: LegacyCreditsTilemapLayout,
        checksum_field: usize,
        fill: u8,
    ) -> Result<bool, CreditsTilemapIoError> {
        validate(self, layout)?;
        for (tail_row, row) in tilemap
            .words()
            .chunks_exact(CreditsTilemap::COLUMNS)
            .enumerate()
            .skip(layout.row_count)
        {
            if row.iter().any(|word| *word != layout.blank_word) {
                return Err(CreditsTilemapIoError::NonblankExpandedRow(tail_row));
            }
        }
        let mut normalized = CreditsTilemap::blank(layout.blank_word);
        normalized.words_mut()[..layout.row_count * CreditsTilemap::COLUMNS]
            .copy_from_slice(&tilemap.words()[..layout.row_count * CreditsTilemap::COLUMNS]);
        let encoded = normalized.encode_rows(layout.blank_word)?;
        let capacity = layout
            .offsets
            .checked_sub(layout.records)
            .ok_or(CreditsTilemapIoError::InvalidLayout)?;
        if encoded.records.len() > capacity {
            return Err(CreditsTilemapIoError::EncodedRecordsTooLarge {
                actual: encoded.records.len(),
                capacity,
            });
        }
        let mut record_region = vec![fill; capacity];
        record_region[..encoded.records.len()].copy_from_slice(&encoded.records);
        let offset_bytes: Vec<_> = encoded.offsets[..layout.row_count]
            .iter()
            .flat_map(|offset| offset.to_le_bytes())
            .collect();
        let mut writes = vec![
            RomWrite {
                offset: layout.records,
                bytes: record_region,
            },
            RomWrite {
                offset: layout.offsets,
                bytes: offset_bytes,
            },
        ];
        if !self.writes_would_change(&writes)? {
            return Ok(false);
        }
        let mut staged = self.rom.clone();
        for write in &writes {
            staged.write(write.offset, &write.bytes)?;
        }
        let checksum = compute_snes_checksum(staged.logical_bytes(), checksum_field)?;
        writes.push(RomWrite {
            offset: checksum_field,
            bytes: checksum.encoded().to_vec(),
        });
        Ok(self.apply_writes("save legacy credits tilemap", &writes)?)
    }
}

fn validate(
    project: &Project,
    layout: LegacyCreditsTilemapLayout,
) -> Result<(), CreditsTilemapIoError> {
    if layout.row_count != 202 {
        return Err(CreditsTilemapIoError::InvalidRowCount(layout.row_count));
    }
    if layout.records >= layout.offsets {
        return Err(CreditsTilemapIoError::InvalidLayout);
    }
    if let Some(identity) = &project.identity
        && identity.mapper != layout.mapper
    {
        return Err(CreditsTilemapIoError::MapperMismatch {
            expected: identity.mapper,
            actual: layout.mapper,
        });
    }
    Ok(())
}
