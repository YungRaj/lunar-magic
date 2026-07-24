//! Bounded interchange format for Lunar Magic's overworld event-number map.

use crate::EventNumberMap;

const MAGIC: &[u8; 8] = b"LMOWMAP1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldEventNumberFileError {
    TooShort,
    Magic,
    InvalidLength(usize),
    Length { actual: usize, expected: usize },
}

impl std::fmt::Display for OverworldEventNumberFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native overworld event-number file: {self:?}"
        )
    }
}

impl std::error::Error for OverworldEventNumberFileError {}

impl EventNumberMap {
    /// Encodes the exact meaningful map prefix in a bounded `LMOWMAP1` artifact.
    ///
    /// # Errors
    ///
    /// Rejects an internal length outside Lunar Magic's 96-through-256-byte range.
    pub fn encode_native_file(&self) -> Result<Vec<u8>, OverworldEventNumberFileError> {
        let length = self.stored_len();
        if !(Self::VANILLA_LEN..=Self::ENTRY_COUNT).contains(&length) {
            return Err(OverworldEventNumberFileError::InvalidLength(length));
        }
        let mut output = Vec::with_capacity(10 + length);
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(
            &u16::try_from(length)
                .map_err(|_| OverworldEventNumberFileError::InvalidLength(length))?
                .to_le_bytes(),
        );
        output.extend_from_slice(self.encode());
        Ok(output)
    }

    /// Decodes one complete `LMOWMAP1` artifact.
    ///
    /// # Errors
    ///
    /// Rejects malformed framing, lengths outside 96 through 256 bytes, truncation, and trailing
    /// bytes.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, OverworldEventNumberFileError> {
        if bytes.len() < 10 {
            return Err(OverworldEventNumberFileError::TooShort);
        }
        if &bytes[..8] != MAGIC {
            return Err(OverworldEventNumberFileError::Magic);
        }
        let length = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        if !(Self::VANILLA_LEN..=Self::ENTRY_COUNT).contains(&length) {
            return Err(OverworldEventNumberFileError::InvalidLength(length));
        }
        let expected = 10 + length;
        if bytes.len() != expected {
            return Err(OverworldEventNumberFileError::Length {
                actual: bytes.len(),
                expected,
            });
        }
        Self::decode(&bytes[10..]).map_err(OverworldEventNumberFileError::InvalidLength)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_and_extended_maps_round_trip_with_exact_framing() {
        for event in [0x20, 0xff] {
            let mut map = EventNumberMap::default();
            map.set(event, event.wrapping_add(7));
            let encoded = map.encode_native_file().unwrap();
            assert_eq!(EventNumberMap::decode_native_file(&encoded).unwrap(), map);
            for end in 0..encoded.len() {
                assert!(EventNumberMap::decode_native_file(&encoded[..end]).is_err());
            }
            let mut trailing = encoded;
            trailing.push(0);
            assert!(EventNumberMap::decode_native_file(&trailing).is_err());
        }
    }
}
