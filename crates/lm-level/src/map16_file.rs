use crate::Map16Page;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16PageFile {
    pub source_page: u16,
    pub page: Map16Page,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16PageFileError {
    WrongLength { actual: usize, expected: usize },
    WrongMagic,
    UnsupportedVersion(u16),
    WrongPageSize(usize),
    Decode(crate::BinaryError),
}

impl fmt::Display for Map16PageFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Map16 page file: {self:?}")
    }
}

impl std::error::Error for Map16PageFileError {}

impl Map16PageFile {
    const MAGIC: &'static [u8; 8] = b"LM16PAGE";
    const VERSION: u16 = 1;
    const HEADER_LEN: usize = 12;
    const GRAPHICS_LEN: usize = Map16Page::TILE_COUNT * 8;
    const ACTS_LIKE_LEN: usize = Map16Page::TILE_COUNT * 2;
    pub const ENCODED_LEN: usize = Self::HEADER_LEN + Self::GRAPHICS_LEN + Self::ACTS_LIKE_LEN;

    /// Encodes exactly one complete page.
    ///
    /// # Errors
    ///
    /// Returns [`Map16PageFileError::WrongPageSize`] for a malformed public page model.
    pub fn encode(&self) -> Result<Vec<u8>, Map16PageFileError> {
        let (graphics, acts_like) = self
            .page
            .encode()
            .map_err(|error| Map16PageFileError::WrongPageSize(error.tiles))?;
        let mut bytes = Vec::with_capacity(Self::ENCODED_LEN);
        bytes.extend_from_slice(Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.source_page.to_le_bytes());
        bytes.extend_from_slice(&graphics);
        bytes.extend_from_slice(&acts_like);
        Ok(bytes)
    }

    /// Decodes an exact standalone Map16 page file.
    ///
    /// # Errors
    ///
    /// Returns [`Map16PageFileError`] for incorrect framing, unsupported versions, or malformed
    /// tile data.
    pub fn decode(bytes: &[u8]) -> Result<Self, Map16PageFileError> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(Map16PageFileError::WrongLength {
                actual: bytes.len(),
                expected: Self::ENCODED_LEN,
            });
        }
        if &bytes[..8] != Self::MAGIC {
            return Err(Map16PageFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([bytes[8], bytes[9]]);
        if version != Self::VERSION {
            return Err(Map16PageFileError::UnsupportedVersion(version));
        }
        let source_page = u16::from_le_bytes([bytes[10], bytes[11]]);
        let graphics_end = Self::HEADER_LEN + Self::GRAPHICS_LEN;
        let page = Map16Page::decode(
            &bytes[Self::HEADER_LEN..graphics_end],
            &bytes[graphics_end..],
        )
        .map_err(Map16PageFileError::Decode)?;
        Ok(Self { source_page, page })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Map16Tile, Subtile};

    #[test]
    fn standalone_page_round_trips_all_fields() {
        let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
        tiles[0x42] = Map16Tile {
            top_left: Subtile(0xe321),
            top_right: Subtile(2),
            bottom_left: Subtile(3),
            bottom_right: Subtile(4),
            acts_like: 0x1234,
        };
        let file = Map16PageFile {
            source_page: 0x10,
            page: Map16Page::new(tiles).unwrap(),
        };
        let encoded = file.encode().unwrap();
        assert_eq!(encoded.len(), Map16PageFile::ENCODED_LEN);
        assert_eq!(Map16PageFile::decode(&encoded).unwrap(), file);
    }

    #[test]
    fn wrong_version_truncation_and_trailing_bytes_are_rejected() {
        let file = Map16PageFile {
            source_page: 0,
            page: Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
        };
        let mut encoded = file.encode().unwrap();
        encoded[8..10].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(
            Map16PageFile::decode(&encoded),
            Err(Map16PageFileError::UnsupportedVersion(2))
        );
        encoded.push(0);
        assert!(matches!(
            Map16PageFile::decode(&encoded),
            Err(Map16PageFileError::WrongLength { .. })
        ));
    }

    #[test]
    fn malformed_public_page_cannot_encode() {
        let file = Map16PageFile {
            source_page: 0,
            page: Map16Page { tiles: vec![] },
        };
        assert_eq!(file.encode(), Err(Map16PageFileError::WrongPageSize(0)));
    }
}
