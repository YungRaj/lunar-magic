//! Lossless editor buffers used by Lunar Magic's overworld event-tilemap runtime.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventTilemapBuffers {
    primary_bytes: [u8; Self::PRIMARY_LEN],
    secondary_high_bytes: [u8; Self::SECONDARY_HIGH_PLANE_LEN],
}

impl Default for EventTilemapBuffers {
    fn default() -> Self {
        Self {
            primary_bytes: [0; Self::PRIMARY_LEN],
            secondary_high_bytes: [0; Self::SECONDARY_HIGH_PLANE_LEN],
        }
    }
}

impl EventTilemapBuffers {
    pub const WORD_COUNT: usize = 0x800;
    pub const PRIMARY_LEN: usize = Self::WORD_COUNT * 2;
    pub const SECONDARY_HIGH_PLANE_LEN: usize = Self::WORD_COUNT;

    #[must_use]
    pub const fn primary_bytes(&self) -> &[u8; Self::PRIMARY_LEN] {
        &self.primary_bytes
    }

    #[must_use]
    pub const fn secondary_high_bytes(&self) -> &[u8; Self::SECONDARY_HIGH_PLANE_LEN] {
        &self.secondary_high_bytes
    }

    pub fn primary_bytes_mut(&mut self) -> &mut [u8; Self::PRIMARY_LEN] {
        &mut self.primary_bytes
    }

    pub fn secondary_high_bytes_mut(&mut self) -> &mut [u8; Self::SECONDARY_HIGH_PLANE_LEN] {
        &mut self.secondary_high_bytes
    }

    /// Reconstructs the two editor planes from the exact native streams.
    ///
    /// The secondary stream contains only the high byte of every word. Low bytes belong to the
    /// caller's base tilemap and are intentionally outside this owned persistence boundary.
    ///
    /// # Errors
    ///
    /// Rejects any plane that is not exactly the native size.
    pub fn decode_streams(
        primary: &[u8],
        secondary_high: &[u8],
    ) -> Result<Self, EventTilemapBufferError> {
        if primary.len() != Self::PRIMARY_LEN
            || secondary_high.len() != Self::SECONDARY_HIGH_PLANE_LEN
        {
            return Err(EventTilemapBufferError::Shape {
                primary: primary.len(),
                secondary_high: secondary_high.len(),
            });
        }
        let mut result = Self::default();
        result.primary_bytes.copy_from_slice(primary);
        result.secondary_high_bytes.copy_from_slice(secondary_high);
        Ok(result)
    }

    #[must_use]
    pub fn encode_primary_stream(&self) -> Vec<u8> {
        self.primary_bytes.to_vec()
    }

    #[must_use]
    pub fn encode_secondary_high_stream(&self) -> Vec<u8> {
        self.secondary_high_bytes.to_vec()
    }

    /// Combines the owned high-byte stream with one external base word plane.
    ///
    /// # Errors
    ///
    /// Rejects a base plane that is not exactly 2,048 words.
    pub fn overlay_secondary_words(
        &self,
        secondary_base: &[u16],
    ) -> Result<Vec<u16>, EventTilemapBufferError> {
        if secondary_base.len() != Self::WORD_COUNT {
            return Err(EventTilemapBufferError::SecondaryBaseLength(
                secondary_base.len(),
            ));
        }
        Ok(secondary_base
            .iter()
            .zip(self.secondary_high_bytes)
            .map(|(base, high)| (base & 0x00ff) | u16::from(high) << 8)
            .collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EventTilemapBufferError {
    Shape {
        primary: usize,
        secondary_high: usize,
    },
    SecondaryBaseLength(usize),
}

impl std::fmt::Display for EventTilemapBufferError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid overworld event-tilemap buffers: {self:?}"
        )
    }
}

impl std::error::Error for EventTilemapBufferError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_streams_round_trip_without_losing_secondary_low_bytes() {
        let mut buffers = EventTilemapBuffers::default();
        buffers.primary_bytes_mut()[0] = 0x12;
        buffers.primary_bytes_mut()[0x7ff] = 0x34;
        buffers.primary_bytes_mut()[0x800] = 0xab;
        buffers.primary_bytes_mut()[0xfff] = 0xcd;
        buffers.secondary_high_bytes_mut()[0] = 0x56;
        buffers.secondary_high_bytes_mut()[0x7ff] = 0xef;
        assert_eq!(
            EventTilemapBuffers::decode_streams(
                &buffers.encode_primary_stream(),
                &buffers.encode_secondary_high_stream(),
            )
            .unwrap(),
            buffers
        );
        assert_eq!(
            buffers
                .overlay_secondary_words(&[0x78; EventTilemapBuffers::WORD_COUNT])
                .unwrap()[0],
            0x5678
        );
    }
}
