use crate::{GraphicsFile4bpp, GraphicsFileError};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsInterchangeFile {
    pub source_slot: u16,
    pub graphics: GraphicsFile4bpp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsInterchangeError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    TooManyTiles(usize),
    LengthOverflow,
    WrongLength { actual: usize, expected: usize },
    Graphics(GraphicsFileError),
}

impl fmt::Display for GraphicsInterchangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid 4bpp graphics interchange file: {self:?}"
        )
    }
}

impl std::error::Error for GraphicsInterchangeError {}

impl GraphicsInterchangeFile {
    const MAGIC: &'static [u8; 8] = b"LMGFX4BP";
    const VERSION: u16 = 1;
    const HEADER_LEN: usize = 16;
    pub const MAX_TILES: usize = 0x1_0000;
    pub const MAX_FILE_LEN: usize =
        Self::HEADER_LEN + Self::MAX_TILES * GraphicsFile4bpp::BYTES_PER_TILE;

    /// Encodes decoded tiles in native SNES planar form with explicit framing.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsInterchangeError`] if the tile count exceeds the portable limit.
    pub fn encode(&self) -> Result<Vec<u8>, GraphicsInterchangeError> {
        if self.graphics.tiles.len() > Self::MAX_TILES {
            return Err(GraphicsInterchangeError::TooManyTiles(
                self.graphics.tiles.len(),
            ));
        }
        let tile_count = u32::try_from(self.graphics.tiles.len())
            .map_err(|_| GraphicsInterchangeError::TooManyTiles(self.graphics.tiles.len()))?;
        let graphics = self
            .graphics
            .encode()
            .map_err(GraphicsInterchangeError::Graphics)?;
        let mut bytes = Vec::with_capacity(Self::HEADER_LEN + graphics.len());
        bytes.extend_from_slice(Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.source_slot.to_le_bytes());
        bytes.extend_from_slice(&tile_count.to_le_bytes());
        bytes.extend_from_slice(&graphics);
        Ok(bytes)
    }

    /// Decodes an exact framed graphics file and rejects trailing data.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsInterchangeError`] for invalid framing, limits, or planar tile data.
    pub fn decode(bytes: &[u8]) -> Result<Self, GraphicsInterchangeError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(GraphicsInterchangeError::Truncated)?;
        if &header[..8] != Self::MAGIC {
            return Err(GraphicsInterchangeError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(GraphicsInterchangeError::UnsupportedVersion(version));
        }
        let source_slot = u16::from_le_bytes([header[10], header[11]]);
        let tile_count = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| GraphicsInterchangeError::TooManyTiles(usize::MAX))?;
        if tile_count > Self::MAX_TILES {
            return Err(GraphicsInterchangeError::TooManyTiles(tile_count));
        }
        let expected = tile_count
            .checked_mul(GraphicsFile4bpp::BYTES_PER_TILE)
            .and_then(|length| length.checked_add(Self::HEADER_LEN))
            .ok_or(GraphicsInterchangeError::LengthOverflow)?;
        if bytes.len() != expected {
            return Err(GraphicsInterchangeError::WrongLength {
                actual: bytes.len(),
                expected,
            });
        }
        let graphics = GraphicsFile4bpp::decode(&bytes[Self::HEADER_LEN..])
            .map_err(GraphicsInterchangeError::Graphics)?;
        Ok(Self {
            source_slot,
            graphics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexedTile;

    fn file() -> GraphicsInterchangeFile {
        GraphicsInterchangeFile {
            source_slot: 0x32,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new(std::array::from_fn(|index| {
                    index.to_le_bytes()[0] & 0x0f
                }))],
            },
        }
    }

    #[test]
    fn decoded_tiles_round_trip_through_planar_file() {
        let file = file();
        assert_eq!(
            GraphicsInterchangeFile::decode(&file.encode().unwrap()).unwrap(),
            file
        );
    }

    #[test]
    fn version_count_and_trailing_data_are_checked() {
        let mut bytes = file().encode().unwrap();
        bytes[8..10].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(
            GraphicsInterchangeFile::decode(&bytes),
            Err(GraphicsInterchangeError::UnsupportedVersion(2))
        );
        bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
        bytes.push(0);
        assert!(matches!(
            GraphicsInterchangeFile::decode(&bytes),
            Err(GraphicsInterchangeError::WrongLength { .. })
        ));
    }

    #[test]
    fn invalid_pixels_are_rejected_instead_of_truncated() {
        let invalid = GraphicsInterchangeFile {
            source_slot: 4,
            graphics: GraphicsFile4bpp {
                tiles: vec![
                    IndexedTile::new([0; IndexedTile::PIXEL_COUNT]),
                    IndexedTile::new([16; IndexedTile::PIXEL_COUNT]),
                ],
            },
        };
        assert_eq!(
            invalid.encode(),
            Err(GraphicsInterchangeError::Graphics(
                GraphicsFileError::PixelOutOfRange {
                    tile: 1,
                    pixel: 0,
                    value: 16,
                }
            ))
        );
    }
}
