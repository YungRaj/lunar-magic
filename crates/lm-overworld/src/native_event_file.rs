//! Lossless exchange format for Lunar Magic's main native event-reveal table.

use crate::{EventRevealTable, EventTableError};

const MAGIC: &[u8; 8] = b"LMOWEVT1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldEventFileError {
    TooShort,
    Magic,
    InvalidCount(usize),
    Length { actual: usize, expected: usize },
    Table(EventTableError),
}

impl std::fmt::Display for OverworldEventFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid native overworld-event file: {self:?}")
    }
}

impl std::error::Error for OverworldEventFileError {}

impl From<EventTableError> for OverworldEventFileError {
    fn from(value: EventTableError) -> Self {
        Self::Table(value)
    }
}

impl EventRevealTable {
    /// Encodes one exact bounded `LMOWEVT1` artifact.
    ///
    /// # Errors
    ///
    /// Rejects empty, excessive, or semantically lossy reveal tables.
    pub fn encode_native_event_file(&self) -> Result<Vec<u8>, OverworldEventFileError> {
        if self.entries.is_empty() {
            return Err(OverworldEventFileError::InvalidCount(0));
        }
        let (sources, destinations) = self.encode()?;
        let mut output = Vec::with_capacity(10 + sources.len() + destinations.len());
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(
            &u16::try_from(self.entries.len())
                .map_err(|_| OverworldEventFileError::InvalidCount(self.entries.len()))?
                .to_le_bytes(),
        );
        output.extend_from_slice(&sources);
        output.extend_from_slice(&destinations);
        Ok(output)
    }

    /// Decodes one complete `LMOWEVT1` artifact.
    ///
    /// # Errors
    ///
    /// Rejects malformed framing, zero/excessive counts, truncation, trailing bytes, and invalid
    /// source tiles.
    pub fn decode_native_event_file(bytes: &[u8]) -> Result<Self, OverworldEventFileError> {
        if bytes.len() < 10 {
            return Err(OverworldEventFileError::TooShort);
        }
        if &bytes[..8] != MAGIC {
            return Err(OverworldEventFileError::Magic);
        }
        let count = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        if count == 0 || count > Self::MAX_ENTRIES {
            return Err(OverworldEventFileError::InvalidCount(count));
        }
        let plane_len = count * 2;
        let expected = 10 + plane_len * 2;
        if bytes.len() != expected {
            return Err(OverworldEventFileError::Length {
                actual: bytes.len(),
                expected,
            });
        }
        let table = Self::decode(&bytes[10..10 + plane_len], &bytes[10 + plane_len..])?;
        table.validate()?;
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventReveal;

    #[test]
    fn exact_round_trip_and_all_truncations() {
        let table = EventRevealTable {
            entries: (0..112)
                .map(|index| EventReveal {
                    source_tile: index,
                    destination_tile: index | 0x100,
                })
                .collect(),
        };
        let encoded = table.encode_native_event_file().unwrap();
        assert_eq!(
            EventRevealTable::decode_native_event_file(&encoded).unwrap(),
            table
        );
        for end in 0..encoded.len() {
            assert!(EventRevealTable::decode_native_event_file(&encoded[..end]).is_err());
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(EventRevealTable::decode_native_event_file(&trailing).is_err());
    }
}
