//! Expanded Layer 1/Layer 2 tilemap stream shared by Lunar Magic's overworld-style editors.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpandedLayerTilemap {
    primary: [u8; Self::PLANE_LEN],
    secondary: [u8; Self::PLANE_LEN],
}

impl Default for ExpandedLayerTilemap {
    fn default() -> Self {
        Self {
            primary: [0; Self::PLANE_LEN],
            secondary: Self::blank_plane(),
        }
    }
}

impl ExpandedLayerTilemap {
    pub const COLUMNS: usize = 32;
    pub const ROWS: usize = 29;
    pub const WORD_COUNT: usize = Self::COLUMNS * Self::ROWS;
    pub const PLANE_LEN: usize = Self::WORD_COUNT * 2;
    pub const BLANK_TILE: u16 = 0x00fc;
    pub const PRIMARY_HEADER: [u8; 4] = [0x50, 0x00, 0x07, 0x3f];
    pub const SECONDARY_HEADER: [u8; 4] = [0x54, 0x00, 0x07, 0x3f];
    pub const TERMINATOR: u8 = 0x80;

    const fn blank_plane() -> [u8; Self::PLANE_LEN] {
        let mut output = [0; Self::PLANE_LEN];
        let mut index = 0;
        while index < Self::PLANE_LEN {
            output[index] = Self::BLANK_TILE.to_le_bytes()[0];
            output[index + 1] = Self::BLANK_TILE.to_le_bytes()[1];
            index += 2;
        }
        output
    }

    #[must_use]
    pub const fn primary_bytes(&self) -> &[u8; Self::PLANE_LEN] {
        &self.primary
    }

    #[must_use]
    pub const fn secondary_bytes(&self) -> &[u8; Self::PLANE_LEN] {
        &self.secondary
    }

    pub fn primary_bytes_mut(&mut self) -> &mut [u8; Self::PLANE_LEN] {
        &mut self.primary
    }

    pub fn secondary_bytes_mut(&mut self) -> &mut [u8; Self::PLANE_LEN] {
        &mut self.secondary
    }

    /// Decodes the exact editor planes.
    ///
    /// # Errors
    ///
    /// Rejects either plane unless it is exactly `$740` bytes.
    pub fn decode_planes(
        primary: &[u8],
        secondary: &[u8],
    ) -> Result<Self, ExpandedLayerTilemapError> {
        Ok(Self {
            primary: primary
                .try_into()
                .map_err(|_| ExpandedLayerTilemapError::PlaneLength {
                    plane: 0,
                    actual: primary.len(),
                })?,
            secondary: secondary.try_into().map_err(|_| {
                ExpandedLayerTilemapError::PlaneLength {
                    plane: 1,
                    actual: secondary.len(),
                }
            })?,
        })
    }

    #[must_use]
    pub fn secondary_is_blank(&self) -> bool {
        self.secondary
            .chunks_exact(2)
            .all(|word| u16::from_le_bytes([word[0], word[1]]) & 0x03ff == Self::BLANK_TILE)
    }

    /// Reproduces Lunar Magic's one- or two-plane stream framing.
    #[must_use]
    pub fn encode_native_stream(&self) -> Vec<u8> {
        let include_secondary = !self.secondary_is_blank();
        let mut output = Vec::with_capacity(
            4 + Self::PLANE_LEN + usize::from(include_secondary) * (4 + Self::PLANE_LEN) + 1,
        );
        output.extend_from_slice(&Self::PRIMARY_HEADER);
        output.extend_from_slice(&self.primary);
        if include_secondary {
            output.extend_from_slice(&Self::SECONDARY_HEADER);
            output.extend_from_slice(&self.secondary);
        }
        output.push(Self::TERMINATOR);
        output
    }

    /// Decodes the exact `$744/$E88` payload followed by `$80`.
    ///
    /// # Errors
    ///
    /// Rejects altered headers, lengths, or terminators.
    pub fn decode_native_stream(bytes: &[u8]) -> Result<Self, ExpandedLayerTilemapError> {
        let one_plane_len = 4 + Self::PLANE_LEN + 1;
        let two_plane_len = 2 * (4 + Self::PLANE_LEN) + 1;
        if bytes.len() != one_plane_len && bytes.len() != two_plane_len {
            return Err(ExpandedLayerTilemapError::StreamLength(bytes.len()));
        }
        if bytes[..4] != Self::PRIMARY_HEADER || bytes[bytes.len() - 1] != Self::TERMINATOR {
            return Err(ExpandedLayerTilemapError::Framing);
        }
        let mut result = Self::default();
        result
            .primary
            .copy_from_slice(&bytes[4..4 + Self::PLANE_LEN]);
        if bytes.len() == two_plane_len {
            let header = 4 + Self::PLANE_LEN;
            if bytes[header..header + 4] != Self::SECONDARY_HEADER {
                return Err(ExpandedLayerTilemapError::Framing);
            }
            result
                .secondary
                .copy_from_slice(&bytes[header + 4..header + 4 + Self::PLANE_LEN]);
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedLayerTilemapError {
    PlaneLength { plane: usize, actual: usize },
    StreamLength(usize),
    Framing,
}

impl std::fmt::Display for ExpandedLayerTilemapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid expanded layer tilemap: {self:?}")
    }
}

impl std::error::Error for ExpandedLayerTilemapError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_secondary_is_omitted_but_materializes_losslessly() {
        let mut tilemap = ExpandedLayerTilemap::default();
        tilemap.primary[0] = 0x44;
        let stream = tilemap.encode_native_stream();
        assert_eq!(stream.len(), 0x745);
        assert_eq!(
            ExpandedLayerTilemap::decode_native_stream(&stream).unwrap(),
            tilemap
        );
    }

    #[test]
    fn nonblank_secondary_uses_the_exact_second_record() {
        let mut tilemap = ExpandedLayerTilemap::default();
        tilemap.secondary[0] = 0x55;
        let stream = tilemap.encode_native_stream();
        assert_eq!(stream.len(), 0xe89);
        assert_eq!(
            &stream[0x744..0x748],
            &ExpandedLayerTilemap::SECONDARY_HEADER
        );
        assert_eq!(
            ExpandedLayerTilemap::decode_native_stream(&stream).unwrap(),
            tilemap
        );
    }
}
