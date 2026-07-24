/// Lossless installed expanded per-level settings record recovered from Lunar Magic.
///
/// The native table uses sixteen little-endian words per level. Meanings that are not yet proven
/// remain accessible as indexed words so save/load never normalizes unknown flags or selectors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedLevelSettingsRecord {
    bytes: [u8; Self::ENCODED_LEN],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpandedLevelSettingsError {
    WrongLength(usize),
    WordOutOfRange(usize),
}

impl std::fmt::Display for ExpandedLevelSettingsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "expanded level settings error: {self:?}")
    }
}

impl std::error::Error for ExpandedLevelSettingsError {}

impl ExpandedLevelSettingsRecord {
    pub const ENCODED_LEN: usize = 0x20;
    pub const WORD_COUNT: usize = 16;

    /// Constructs a record from an already shape-checked byte array.
    #[must_use]
    pub const fn from_encoded(bytes: [u8; Self::ENCODED_LEN]) -> Self {
        Self { bytes }
    }

    /// Decodes one exact native record without assigning meanings to unknown bits.
    ///
    /// # Errors
    ///
    /// Returns [`ExpandedLevelSettingsError::WrongLength`] unless exactly 32 bytes are supplied.
    pub fn decode(bytes: &[u8]) -> Result<Self, ExpandedLevelSettingsError> {
        let bytes = bytes
            .try_into()
            .map_err(|_| ExpandedLevelSettingsError::WrongLength(bytes.len()))?;
        Ok(Self { bytes })
    }

    #[must_use]
    pub const fn encoded(&self) -> &[u8; Self::ENCODED_LEN] {
        &self.bytes
    }

    /// Reads one native little-endian word.
    ///
    /// # Errors
    ///
    /// Returns [`ExpandedLevelSettingsError::WordOutOfRange`] for indexes above 15.
    pub fn word(&self, index: usize) -> Result<u16, ExpandedLevelSettingsError> {
        let offset = word_offset(index)?;
        Ok(u16::from_le_bytes([
            self.bytes[offset],
            self.bytes[offset + 1],
        ]))
    }

    /// Replaces one native word while preserving every other byte.
    ///
    /// # Errors
    ///
    /// Returns [`ExpandedLevelSettingsError::WordOutOfRange`] for indexes above 15.
    pub fn set_word(&mut self, index: usize, value: u16) -> Result<(), ExpandedLevelSettingsError> {
        let offset = word_offset(index)?;
        self.bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
}

impl From<crate::ExpandedLevelHeader> for ExpandedLevelSettingsRecord {
    fn from(header: crate::ExpandedLevelHeader) -> Self {
        Self {
            bytes: header.encode(),
        }
    }
}

impl From<ExpandedLevelSettingsRecord> for crate::ExpandedLevelHeader {
    fn from(record: ExpandedLevelSettingsRecord) -> Self {
        Self::decode(&record.bytes).expect("expanded settings and header shapes are identical")
    }
}

impl From<&ExpandedLevelSettingsRecord> for crate::ExpandedLevelHeader {
    fn from(record: &ExpandedLevelSettingsRecord) -> Self {
        Self::decode(&record.bytes).expect("expanded settings and header shapes are identical")
    }
}

fn word_offset(index: usize) -> Result<usize, ExpandedLevelSettingsError> {
    (index < ExpandedLevelSettingsRecord::WORD_COUNT)
        .then_some(index * 2)
        .ok_or(ExpandedLevelSettingsError::WordOutOfRange(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_edits_independently_and_round_trips_exactly() {
        let source = std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap());
        let mut record = ExpandedLevelSettingsRecord::decode(&source).unwrap();
        for index in 0..ExpandedLevelSettingsRecord::WORD_COUNT {
            let before = *record.encoded();
            record
                .set_word(index, u16::try_from(index).unwrap() | 0xa500)
                .unwrap();
            for (byte, previous) in before.iter().copied().enumerate() {
                if byte / 2 != index {
                    assert_eq!(record.encoded()[byte], previous);
                }
            }
        }
        assert!(record.word(16).is_err());
        assert!(record.set_word(16, 0).is_err());
        assert_eq!(
            ExpandedLevelSettingsRecord::decode(record.encoded()).unwrap(),
            record
        );
    }

    #[test]
    fn portable_header_and_installed_record_convert_without_normalizing_unknown_words() {
        let source =
            std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap().wrapping_mul(17));
        let record = ExpandedLevelSettingsRecord::decode(&source).unwrap();
        let header = crate::ExpandedLevelHeader::from(&record);
        assert_eq!(header.encode(), source);
        assert_eq!(ExpandedLevelSettingsRecord::from(header), record);
    }
}
