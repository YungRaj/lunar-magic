//! Exact interchange format for Lunar Magic's two overworld event-tilemap buffers.

use crate::{EventTilemapBufferError, EventTilemapBuffers};

const MAGIC: &[u8; 8] = b"LMOWTIL1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventTilemapFileError {
    Length { actual: usize, expected: usize },
    Magic,
    Buffers(EventTilemapBufferError),
}

impl std::fmt::Display for EventTilemapFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid native event-tilemap file: {self:?}")
    }
}

impl std::error::Error for EventTilemapFileError {}

impl From<EventTilemapBufferError> for EventTilemapFileError {
    fn from(value: EventTilemapBufferError) -> Self {
        Self::Buffers(value)
    }
}

impl EventTilemapBuffers {
    pub const FILE_LEN: usize = 8 + Self::PRIMARY_LEN + Self::SECONDARY_HIGH_PLANE_LEN;

    #[must_use]
    pub fn encode_native_file(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(Self::FILE_LEN);
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&self.encode_primary_stream());
        output.extend_from_slice(&self.encode_secondary_high_stream());
        output
    }

    /// Decodes one complete `LMOWTIL1` artifact.
    ///
    /// # Errors
    ///
    /// Rejects wrong magic, truncation, trailing data, or malformed native planes.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, EventTilemapFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(EventTilemapFileError::Length {
                actual: bytes.len(),
                expected: Self::FILE_LEN,
            });
        }
        if &bytes[..8] != MAGIC {
            return Err(EventTilemapFileError::Magic);
        }
        Ok(Self::decode_streams(
            &bytes[8..8 + Self::PRIMARY_LEN],
            &bytes[8 + Self::PRIMARY_LEN..],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_file_round_trips_and_rejects_all_truncations() {
        let mut buffers = EventTilemapBuffers::default();
        buffers.primary_bytes_mut()[7] = 0x12;
        buffers.secondary_high_bytes_mut()[9] = 0xab;
        let bytes = buffers.encode_native_file();
        assert_eq!(
            EventTilemapBuffers::decode_native_file(&bytes).unwrap(),
            buffers
        );
        for end in 0..bytes.len() {
            assert!(EventTilemapBuffers::decode_native_file(&bytes[..end]).is_err());
        }
    }
}
