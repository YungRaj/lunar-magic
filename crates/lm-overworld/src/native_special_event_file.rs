//! Exact interchange format for the 24 special-event reveal records.

use crate::{SpecialEventRevealError, SpecialEventRevealTable};

const MAGIC: &[u8; 8] = b"LMOWSPC1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpecialEventRevealFileError {
    Length { actual: usize, expected: usize },
    Magic,
    Table(SpecialEventRevealError),
}

impl std::fmt::Display for SpecialEventRevealFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native special-event reveal file: {self:?}"
        )
    }
}

impl std::error::Error for SpecialEventRevealFileError {}

impl From<SpecialEventRevealError> for SpecialEventRevealFileError {
    fn from(value: SpecialEventRevealError) -> Self {
        Self::Table(value)
    }
}

impl SpecialEventRevealTable {
    pub const FILE_LEN: usize = 8 + Self::WORD_PLANE_LEN * 2 + Self::ENTRY_COUNT;

    /// Encodes one exact `LMOWSPC1` artifact.
    ///
    /// # Errors
    ///
    /// Rejects source values that cannot semantically reopen through Lunar Magic.
    pub fn encode_native_file(&self) -> Result<Vec<u8>, SpecialEventRevealFileError> {
        let planes = self.encode()?;
        let mut output = Vec::with_capacity(Self::FILE_LEN);
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&planes.sources);
        output.extend_from_slice(&planes.destinations);
        output.extend_from_slice(&planes.directions);
        Ok(output)
    }

    /// Decodes one complete `LMOWSPC1` artifact.
    ///
    /// # Errors
    ///
    /// Rejects wrong magic, truncation, trailing bytes, or a malformed native table.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, SpecialEventRevealFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(SpecialEventRevealFileError::Length {
                actual: bytes.len(),
                expected: Self::FILE_LEN,
            });
        }
        if &bytes[..8] != MAGIC {
            return Err(SpecialEventRevealFileError::Magic);
        }
        let sources_end = 8 + Self::WORD_PLANE_LEN;
        let destinations_end = sources_end + Self::WORD_PLANE_LEN;
        Ok(Self::decode(
            &bytes[8..sources_end],
            &bytes[sources_end..destinations_end],
            &bytes[destinations_end..],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventReveal;

    #[test]
    fn exact_framing_round_trips_and_rejects_every_truncation() {
        let mut table = SpecialEventRevealTable::default();
        table.reveals[23] = EventReveal {
            source_tile: 0x700,
            destination_tile: 0xabcd,
        };
        table.directions[23] = 0xfe;
        let encoded = table.encode_native_file().unwrap();
        assert_eq!(
            SpecialEventRevealTable::decode_native_file(&encoded).unwrap(),
            table
        );
        for end in 0..encoded.len() {
            assert!(SpecialEventRevealTable::decode_native_file(&encoded[..end]).is_err());
        }
    }
}
