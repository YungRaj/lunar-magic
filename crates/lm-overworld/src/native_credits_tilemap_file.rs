use crate::{CreditsTilemap, CreditsTilemapError};

const MAGIC: &[u8; 8] = b"LMCREDT1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreditsTilemapFileError {
    Length { actual: usize, expected: usize },
    Magic,
    Tilemap(CreditsTilemapError),
}

impl std::fmt::Display for CreditsTilemapFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid credits tilemap file: {self:?}")
    }
}

impl std::error::Error for CreditsTilemapFileError {}

impl From<CreditsTilemapError> for CreditsTilemapFileError {
    fn from(value: CreditsTilemapError) -> Self {
        Self::Tilemap(value)
    }
}

impl CreditsTilemap {
    pub const FILE_LEN: usize = 8 + Self::WORD_COUNT * 2;

    #[must_use]
    pub fn encode_native_file(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(Self::FILE_LEN);
        output.extend_from_slice(MAGIC);
        for word in self.words() {
            output.extend_from_slice(&word.to_le_bytes());
        }
        output
    }

    /// Decodes one exact allocation-independent `LMCREDT1` image.
    ///
    /// # Errors
    ///
    /// Rejects wrong magic, truncation, trailing bytes, or an invalid tilemap shape.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, CreditsTilemapFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(CreditsTilemapFileError::Length {
                actual: bytes.len(),
                expected: Self::FILE_LEN,
            });
        }
        if &bytes[..8] != MAGIC {
            return Err(CreditsTilemapFileError::Magic);
        }
        let words = bytes[8..]
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect();
        Ok(Self::new(words)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_file_round_trips_and_rejects_trailing_bytes() {
        let mut tilemap = CreditsTilemap::blank(0x38fc);
        tilemap.words_mut()[123] = 0xabcd;
        let mut bytes = tilemap.encode_native_file();
        assert_eq!(CreditsTilemap::decode_native_file(&bytes).unwrap(), tilemap);
        bytes.push(0);
        assert!(CreditsTilemap::decode_native_file(&bytes).is_err());
    }
}
