//! Lossless seven-record expanded overworld settings boundary.

use crate::{ExpandedLevelSettingsError, ExpandedLevelSettingsRecord};

const MAGIC: &[u8; 8] = b"LMOWSET1";
const VERSION: u16 = 1;
const SUBMAP_COUNT_U16: u16 = 7;
const HEADER_LEN: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedOverworldSettings {
    pub records: [ExpandedLevelSettingsRecord; Self::SUBMAP_COUNT],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedOverworldSettingsError {
    WrongMagic,
    UnsupportedVersion(u16),
    WrongCount(u16),
    WrongLength { expected: usize, actual: usize },
    Record(ExpandedLevelSettingsError),
}

impl std::fmt::Display for ExpandedOverworldSettingsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "expanded overworld settings error: {self:?}")
    }
}

impl std::error::Error for ExpandedOverworldSettingsError {}

impl From<ExpandedLevelSettingsError> for ExpandedOverworldSettingsError {
    fn from(value: ExpandedLevelSettingsError) -> Self {
        Self::Record(value)
    }
}

impl ExpandedOverworldSettings {
    pub const SUBMAP_COUNT: usize = 7;
    pub const ENCODED_LEN: usize =
        HEADER_LEN + Self::SUBMAP_COUNT * ExpandedLevelSettingsRecord::ENCODED_LEN;

    #[must_use]
    pub fn encode_file(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::ENCODED_LEN);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&SUBMAP_COUNT_U16.to_le_bytes());
        for record in &self.records {
            bytes.extend_from_slice(record.encoded());
        }
        bytes
    }

    /// Decodes one exact `LMOWSET1` file.
    ///
    /// # Errors
    ///
    /// Rejects wrong framing, versions, counts, lengths, or malformed nested records.
    pub fn decode_file(bytes: &[u8]) -> Result<Self, ExpandedOverworldSettingsError> {
        let header =
            bytes
                .get(..HEADER_LEN)
                .ok_or(ExpandedOverworldSettingsError::WrongLength {
                    expected: Self::ENCODED_LEN,
                    actual: bytes.len(),
                })?;
        if &header[..8] != MAGIC {
            return Err(ExpandedOverworldSettingsError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != VERSION {
            return Err(ExpandedOverworldSettingsError::UnsupportedVersion(version));
        }
        let count = u16::from_le_bytes([header[10], header[11]]);
        if usize::from(count) != Self::SUBMAP_COUNT {
            return Err(ExpandedOverworldSettingsError::WrongCount(count));
        }
        if bytes.len() != Self::ENCODED_LEN {
            return Err(ExpandedOverworldSettingsError::WrongLength {
                expected: Self::ENCODED_LEN,
                actual: bytes.len(),
            });
        }
        let records = bytes[HEADER_LEN..]
            .chunks_exact(ExpandedLevelSettingsRecord::ENCODED_LEN)
            .map(ExpandedLevelSettingsRecord::decode)
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|records: Vec<_>| {
                ExpandedOverworldSettingsError::WrongCount(
                    u16::try_from(records.len()).unwrap_or(u16::MAX),
                )
            })?;
        Ok(Self { records })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_lossless_records_round_trip_and_require_exact_framing() {
        let records = std::array::from_fn(|index| {
            ExpandedLevelSettingsRecord::decode(&[u8::try_from(index).unwrap(); 32]).unwrap()
        });
        let settings = ExpandedOverworldSettings { records };
        let encoded = settings.encode_file();
        assert_eq!(encoded.len(), ExpandedOverworldSettings::ENCODED_LEN);
        assert_eq!(
            ExpandedOverworldSettings::decode_file(&encoded).unwrap(),
            settings
        );
        for length in [0, 11, encoded.len() - 1, encoded.len() + 1] {
            let mut malformed = encoded.clone();
            malformed.resize(length, 0);
            assert!(ExpandedOverworldSettings::decode_file(&malformed).is_err());
        }
    }
}
