use crate::{ExpandedLayerTilemap, ExpandedLayerTilemapError};

const MAGIC: &[u8; 8] = b"LMOWLYR1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpandedLayerTilemapFileError {
    Length { actual: usize, expected: usize },
    Magic,
    Tilemap(ExpandedLayerTilemapError),
}

impl std::fmt::Display for ExpandedLayerTilemapFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid expanded layer tilemap file: {self:?}")
    }
}

impl std::error::Error for ExpandedLayerTilemapFileError {}

impl From<ExpandedLayerTilemapError> for ExpandedLayerTilemapFileError {
    fn from(value: ExpandedLayerTilemapError) -> Self {
        Self::Tilemap(value)
    }
}

impl ExpandedLayerTilemap {
    pub const FILE_LEN: usize = 8 + 2 * Self::PLANE_LEN;

    #[must_use]
    pub fn encode_native_file(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(Self::FILE_LEN);
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(self.primary_bytes());
        output.extend_from_slice(self.secondary_bytes());
        output
    }

    /// Decodes one exact allocation-independent `LMOWLYR1` image.
    ///
    /// # Errors
    ///
    /// Rejects wrong magic, truncation, trailing bytes, or malformed plane sizes.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, ExpandedLayerTilemapFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(ExpandedLayerTilemapFileError::Length {
                actual: bytes.len(),
                expected: Self::FILE_LEN,
            });
        }
        if &bytes[..8] != MAGIC {
            return Err(ExpandedLayerTilemapFileError::Magic);
        }
        Ok(Self::decode_planes(
            &bytes[8..8 + Self::PLANE_LEN],
            &bytes[8 + Self::PLANE_LEN..],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_file_preserves_an_explicit_blank_secondary_plane() {
        let mut tilemap = ExpandedLayerTilemap::default();
        tilemap.primary_bytes_mut()[19] = 0x44;
        let bytes = tilemap.encode_native_file();
        assert_eq!(
            ExpandedLayerTilemap::decode_native_file(&bytes).unwrap(),
            tilemap
        );
    }
}
