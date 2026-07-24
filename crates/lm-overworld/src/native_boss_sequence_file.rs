use crate::{BossSequenceMessageTable, BossSequenceTableError};

const MAGIC: &[u8; 8] = b"LMOWBOS1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BossSequenceFileError {
    Length { actual: usize, expected: usize },
    Magic,
    Table(BossSequenceTableError),
}

impl std::fmt::Display for BossSequenceFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid native boss-sequence file: {self:?}")
    }
}

impl std::error::Error for BossSequenceFileError {}

impl From<BossSequenceTableError> for BossSequenceFileError {
    fn from(value: BossSequenceTableError) -> Self {
        Self::Table(value)
    }
}

impl BossSequenceMessageTable {
    pub const FILE_LEN: usize = 8 + Self::MESSAGE_COUNT * crate::BossSequenceMessage::ENCODED_LEN;

    #[must_use]
    pub fn encode_native_file(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(Self::FILE_LEN);
        output.extend_from_slice(MAGIC);
        for message in &self.messages {
            output.extend_from_slice(message.encoded());
        }
        output
    }

    /// Decodes one exact `LMOWBOS1` file.
    ///
    /// # Errors
    ///
    /// Rejects wrong magic, truncation, trailing bytes, or malformed message slices.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, BossSequenceFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(BossSequenceFileError::Length {
                actual: bytes.len(),
                expected: Self::FILE_LEN,
            });
        }
        if &bytes[..8] != MAGIC {
            return Err(BossSequenceFileError::Magic);
        }
        let mut rows = bytes[8..].chunks_exact(crate::BossSequenceMessage::ENCODED_LEN);
        Ok(Self {
            messages: std::array::from_fn(|_| {
                crate::BossSequenceMessage::decode(rows.next().unwrap_or(&[]))
                    .unwrap_or(crate::BossSequenceMessage([Self::BLANK_GLYPH; 192]))
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_file_round_trips_and_rejects_every_truncation() {
        let mut table = BossSequenceMessageTable::default();
        table.messages[3].0[17] = 0x44;
        let bytes = table.encode_native_file();
        assert_eq!(
            BossSequenceMessageTable::decode_native_file(&bytes).unwrap(),
            table
        );
        for end in 0..bytes.len() {
            assert!(BossSequenceMessageTable::decode_native_file(&bytes[..end]).is_err());
        }
    }
}
