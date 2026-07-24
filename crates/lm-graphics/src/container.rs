use crate::{IndexedTile, decode_4bpp_tile, encode_4bpp_tile};
use std::fmt;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GraphicsFile4bpp {
    pub tiles: Vec<IndexedTile>,
}

impl GraphicsFile4bpp {
    pub const BYTES_PER_TILE: usize = 32;

    /// Decodes a raw SNES 4bpp graphics file.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsFileError::PartialTile`] unless the length is a multiple of 32 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, GraphicsFileError> {
        if bytes.len() % Self::BYTES_PER_TILE != 0 {
            return Err(GraphicsFileError::PartialTile(bytes.len()));
        }
        let tiles = bytes
            .chunks_exact(Self::BYTES_PER_TILE)
            .map(decode_4bpp_tile)
            .collect::<Result<Vec<_>, _>>()
            .map_err(GraphicsFileError::PartialTile)?;
        Ok(Self { tiles })
    }

    /// Encodes every tile without silently truncating invalid pixel indexes.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsFileError::PixelOutOfRange`] with the tile and pixel location.
    pub fn encode(&self) -> Result<Vec<u8>, GraphicsFileError> {
        let encoded_len = encoded_4bpp_len(self.tiles.len())?;
        let mut bytes = Vec::with_capacity(encoded_len);
        for (tile_index, tile) in self.tiles.iter().enumerate() {
            let encoded = encode_4bpp_tile(tile).map_err(|error| match error {
                GraphicsFileError::PixelOutOfRange { pixel, value, .. } => {
                    GraphicsFileError::PixelOutOfRange {
                        tile: tile_index,
                        pixel,
                        value,
                    }
                }
                other => other,
            })?;
            bytes.extend_from_slice(&encoded);
        }
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JoinedGraphics {
    pub files: Vec<Vec<u8>>,
}

impl JoinedGraphics {
    /// Splits a joined graphics image using an explicit file-size table.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsFileError::JoinedSizeMismatch`] if sizes overflow or do not consume the
    /// complete input.
    pub fn split(bytes: &[u8], file_sizes: &[usize]) -> Result<Self, GraphicsFileError> {
        let expected = joined_len(file_sizes)?;
        if expected != bytes.len() {
            return Err(GraphicsFileError::JoinedSizeMismatch {
                expected,
                actual: bytes.len(),
            });
        }
        let mut cursor = 0;
        let files = file_sizes
            .iter()
            .map(|size| {
                let end = cursor + size;
                let file = bytes[cursor..end].to_vec();
                cursor = end;
                file
            })
            .collect();
        Ok(Self { files })
    }

    /// Joins all graphics files after exact aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsFileError::JoinedSizeOverflow`] if the sum of public file lengths cannot
    /// be represented by the target platform.
    pub fn join(&self) -> Result<Vec<u8>, GraphicsFileError> {
        let mut joined = Vec::with_capacity(checked_joined_len(
            self.files.iter().map(Vec::len),
            self.files.len(),
        )?);
        for file in &self.files {
            joined.extend_from_slice(file);
        }
        Ok(joined)
    }

    #[must_use]
    pub fn file_sizes(&self) -> Vec<usize> {
        self.files.iter().map(Vec::len).collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsFileError {
    PartialTile(usize),
    SizeOverflow {
        tiles: usize,
    },
    PixelOutOfRange {
        tile: usize,
        pixel: usize,
        value: u8,
    },
    JoinedSizeMismatch {
        expected: usize,
        actual: usize,
    },
    JoinedSizeOverflow {
        files: usize,
    },
}

fn joined_len(file_sizes: &[usize]) -> Result<usize, GraphicsFileError> {
    checked_joined_len(file_sizes.iter().copied(), file_sizes.len())
}

fn checked_joined_len(
    lengths: impl IntoIterator<Item = usize>,
    files: usize,
) -> Result<usize, GraphicsFileError> {
    lengths.into_iter().try_fold(0_usize, |total, size| {
        total
            .checked_add(size)
            .ok_or(GraphicsFileError::JoinedSizeOverflow { files })
    })
}

impl fmt::Display for GraphicsFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid graphics file: {self:?}")
    }
}

impl std::error::Error for GraphicsFileError {}

fn encoded_4bpp_len(tiles: usize) -> Result<usize, GraphicsFileError> {
    tiles
        .checked_mul(GraphicsFile4bpp::BYTES_PER_TILE)
        .ok_or(GraphicsFileError::SizeOverflow { tiles })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planar_file_round_trips() {
        let bytes: Vec<_> = (0_usize..64).map(|index| index.to_le_bytes()[0]).collect();
        let file = GraphicsFile4bpp::decode(&bytes).unwrap();
        assert_eq!(file.tiles.len(), 2);
        assert_eq!(file.encode().unwrap(), bytes);
        assert!(GraphicsFile4bpp::decode(&[0; 31]).is_err());
    }

    #[test]
    fn planar_file_rejects_invalid_pixels_with_their_location() {
        let file = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; 64]), IndexedTile::new([31; 64])],
        };
        assert_eq!(
            file.encode(),
            Err(GraphicsFileError::PixelOutOfRange {
                tile: 1,
                pixel: 0,
                value: 31,
            })
        );
    }

    #[test]
    fn planar_file_size_is_preflighted_without_saturating() {
        let maximum = usize::MAX / GraphicsFile4bpp::BYTES_PER_TILE;
        assert_eq!(
            encoded_4bpp_len(maximum).unwrap(),
            maximum * GraphicsFile4bpp::BYTES_PER_TILE
        );
        assert_eq!(
            encoded_4bpp_len(maximum + 1),
            Err(GraphicsFileError::SizeOverflow { tiles: maximum + 1 })
        );
    }

    #[test]
    fn joined_file_split_is_exact() {
        let joined = (0_u8..10).collect::<Vec<_>>();
        let files = JoinedGraphics::split(&joined, &[3, 0, 7]).unwrap();
        assert_eq!(files.file_sizes(), [3, 0, 7]);
        assert_eq!(files.join().unwrap(), joined);
        assert!(JoinedGraphics::split(&joined, &[3, 6]).is_err());
    }

    #[test]
    fn joined_length_overflow_is_typed_in_both_directions() {
        let sizes = [usize::MAX, 1];
        assert_eq!(
            joined_len(&sizes),
            Err(GraphicsFileError::JoinedSizeOverflow { files: 2 })
        );
        assert_eq!(
            JoinedGraphics::split(&[], &sizes),
            Err(GraphicsFileError::JoinedSizeOverflow { files: 2 })
        );
    }
}
