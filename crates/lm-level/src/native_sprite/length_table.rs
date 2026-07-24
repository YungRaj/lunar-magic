#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpriteLengthTable {
    lengths: [u8; Self::ENCODED_LEN],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpriteLengthTableError {
    TableOutOfRange(u8),
    RecordTooShort(u8),
}

impl std::fmt::Display for SpriteLengthTableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid sprite length table edit: {self:?}")
    }
}

impl std::error::Error for SpriteLengthTableError {}

impl SpriteLengthTable {
    pub const ENCODED_LEN: usize = 4 * 256;

    #[must_use]
    pub const fn standard() -> Self {
        Self {
            lengths: [3; Self::ENCODED_LEN],
        }
    }

    /// Decodes the four 256-entry runtime length tables.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless exactly 1,024 entries are provided.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        Ok(Self {
            lengths: bytes.try_into().map_err(|_| bytes.len())?,
        })
    }

    #[must_use]
    pub const fn encoded(&self) -> &[u8; Self::ENCODED_LEN] {
        &self.lengths
    }

    #[must_use]
    pub fn record_len(&self, bytes: &[u8]) -> Option<usize> {
        let first = usize::from(*bytes.first()?);
        let id = usize::from(*bytes.get(2)?);
        let table = first >> 2 & 3;
        let len = usize::from(self.lengths[table * 256 + id]);
        (len >= 3).then_some(len)
    }

    /// Replaces one entry without normalizing its table selector or record length.
    ///
    /// # Errors
    ///
    /// Returns [`SpriteLengthTableError`] for a selector outside the four recovered tables or a
    /// length below the three-byte base record.
    pub fn set(&mut self, table: u8, sprite_id: u8, len: u8) -> Result<(), SpriteLengthTableError> {
        if table >= 4 {
            return Err(SpriteLengthTableError::TableOutOfRange(table));
        }
        if len < 3 {
            return Err(SpriteLengthTableError::RecordTooShort(len));
        }
        self.lengths[usize::from(table) * 256 + usize::from(sprite_id)] = len;
        Ok(())
    }
}

impl Default for SpriteLengthTable {
    fn default() -> Self {
        Self::standard()
    }
}
