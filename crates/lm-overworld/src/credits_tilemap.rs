//! Row-deduplicated credits tilemap recovered from Lunar Magic 3.63.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreditsTilemap {
    words: Vec<u16>,
}

impl CreditsTilemap {
    pub const COLUMNS: usize = 32;
    pub const ROWS: usize = 256;
    pub const WORD_COUNT: usize = Self::COLUMNS * Self::ROWS;
    pub const OFFSET_TABLE_LEN: usize = Self::ROWS * 2;

    /// Creates the exact 256×32 editor tilemap.
    ///
    /// # Errors
    ///
    /// Rejects any other word count.
    pub fn new(words: Vec<u16>) -> Result<Self, CreditsTilemapError> {
        if words.len() != Self::WORD_COUNT {
            return Err(CreditsTilemapError::WordCount(words.len()));
        }
        Ok(Self { words })
    }

    #[must_use]
    pub fn blank(blank_word: u16) -> Self {
        Self {
            words: vec![blank_word; Self::WORD_COUNT],
        }
    }

    #[must_use]
    pub fn words(&self) -> &[u16] {
        &self.words
    }

    pub fn words_mut(&mut self) -> &mut [u16] {
        &mut self.words
    }

    /// Encodes the fixed 256-entry offset table and deduplicated variable row records.
    ///
    /// A row record is `$FF` when blank. Otherwise it contains the first nonblank column,
    /// encoded byte count minus one, and the inclusive nonblank word span.
    ///
    /// # Errors
    ///
    /// Rejects a record stream whose 16-bit offsets would overflow.
    pub fn encode_rows(&self, blank_word: u16) -> Result<EncodedCreditsRows, CreditsTilemapError> {
        let mut offsets = [0_u16; Self::ROWS];
        let mut records = Vec::<u8>::new();
        let mut unique: Vec<(Vec<u16>, u16)> = Vec::new();
        for (row_index, row) in self.words.chunks_exact(Self::COLUMNS).enumerate() {
            if let Some((_, offset)) = unique.iter().find(|(candidate, _)| candidate == row) {
                offsets[row_index] = *offset;
                continue;
            }
            let offset =
                u16::try_from(records.len()).map_err(|_| CreditsTilemapError::OffsetOverflow)?;
            offsets[row_index] = offset;
            unique.push((row.to_vec(), offset));
            let start = row.iter().position(|word| *word != blank_word);
            let end = row.iter().rposition(|word| *word != blank_word);
            let (Some(start), Some(end)) = (start, end) else {
                records.push(0xff);
                continue;
            };
            let byte_len = (end - start + 1) * 2;
            records.push(u8::try_from(start).map_err(|_| CreditsTilemapError::OffsetOverflow)?);
            records
                .push(u8::try_from(byte_len - 1).map_err(|_| CreditsTilemapError::OffsetOverflow)?);
            for word in &row[start..=end] {
                records.extend_from_slice(&word.to_le_bytes());
            }
        }
        Ok(EncodedCreditsRows { offsets, records })
    }

    /// Decodes 202 legacy or 256 expanded row offsets.
    ///
    /// Missing legacy rows materialize as blank. Duplicate offsets deliberately share records.
    ///
    /// # Errors
    ///
    /// Rejects unsupported row counts, out-of-range offsets, malformed spans, or truncated words.
    pub fn decode_rows(
        offsets: &[u16],
        records: &[u8],
        blank_word: u16,
    ) -> Result<Self, CreditsTilemapError> {
        if offsets.len() != 202 && offsets.len() != Self::ROWS {
            return Err(CreditsTilemapError::RowCount(offsets.len()));
        }
        let mut result = Self::blank(blank_word);
        for (row_index, offset) in offsets.iter().copied().enumerate() {
            let start = usize::from(offset);
            let marker = *records
                .get(start)
                .ok_or(CreditsTilemapError::RecordOffset {
                    row: row_index,
                    offset: start,
                })?;
            if marker == 0xff {
                continue;
            }
            let encoded_len = usize::from(
                *records
                    .get(start + 1)
                    .ok_or(CreditsTilemapError::TruncatedRecord(row_index))?,
            ) + 1;
            if encoded_len == 0 || encoded_len & 1 != 0 {
                return Err(CreditsTilemapError::RecordLength {
                    row: row_index,
                    encoded_len,
                });
            }
            let column = usize::from(marker);
            let word_count = encoded_len / 2;
            if column + word_count > Self::COLUMNS {
                return Err(CreditsTilemapError::RecordSpan {
                    row: row_index,
                    column,
                    word_count,
                });
            }
            let payload = records
                .get(start + 2..start + 2 + encoded_len)
                .ok_or(CreditsTilemapError::TruncatedRecord(row_index))?;
            let destination = row_index * Self::COLUMNS + column;
            for (index, word) in payload.chunks_exact(2).enumerate() {
                result.words[destination + index] = u16::from_le_bytes([word[0], word[1]]);
            }
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedCreditsRows {
    pub offsets: [u16; CreditsTilemap::ROWS],
    pub records: Vec<u8>,
}

impl EncodedCreditsRows {
    #[must_use]
    pub fn offset_bytes(&self) -> Vec<u8> {
        self.offsets
            .iter()
            .flat_map(|offset| offset.to_le_bytes())
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreditsTilemapError {
    WordCount(usize),
    RowCount(usize),
    OffsetOverflow,
    RecordOffset {
        row: usize,
        offset: usize,
    },
    TruncatedRecord(usize),
    RecordLength {
        row: usize,
        encoded_len: usize,
    },
    RecordSpan {
        row: usize,
        column: usize,
        word_count: usize,
    },
}

impl std::fmt::Display for CreditsTilemapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid credits tilemap: {self:?}")
    }
}

impl std::error::Error for CreditsTilemapError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_trim_deduplicate_and_round_trip() {
        let blank = 0x38fc;
        let mut tilemap = CreditsTilemap::blank(blank);
        tilemap.words[3] = 0x1234;
        tilemap.words[31] = 0x5678;
        tilemap.words[CreditsTilemap::COLUMNS + 3] = 0x1234;
        tilemap.words[CreditsTilemap::COLUMNS + 31] = 0x5678;
        let encoded = tilemap.encode_rows(blank).unwrap();
        assert_eq!(encoded.offsets[0], encoded.offsets[1]);
        assert_eq!(
            CreditsTilemap::decode_rows(&encoded.offsets, &encoded.records, blank).unwrap(),
            tilemap
        );
    }

    #[test]
    fn legacy_shape_leaves_the_unrepresented_tail_blank() {
        let blank = 0x38fc;
        let offsets = [0_u16; 202];
        let decoded = CreditsTilemap::decode_rows(&offsets, &[0xff], blank).unwrap();
        assert!(
            decoded.words[202 * CreditsTilemap::COLUMNS..]
                .iter()
                .all(|word| *word == blank)
        );
    }
}
