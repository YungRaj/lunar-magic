use crate::{
    EventTileChange, OverworldEndpoint, OverworldMessage, OverworldMetadata, OverworldPathGraph,
    OverworldSprite,
};
use lm_graphics::{ExAnimationSet, Palette};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Submap {
    Main,
    YoshiIsland,
    VanillaDome,
    ForestOfIllusion,
    ValleyOfBowser,
    SpecialWorld,
    StarWorld,
}

impl Submap {
    #[must_use]
    pub const fn encoded(self) -> u8 {
        match self {
            Self::Main => 0,
            Self::YoshiIsland => 1,
            Self::VanillaDome => 2,
            Self::ForestOfIllusion => 3,
            Self::ValleyOfBowser => 4,
            Self::SpecialWorld => 5,
            Self::StarWorld => 6,
        }
    }

    #[must_use]
    pub const fn decode(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Main),
            1 => Some(Self::YoshiIsland),
            2 => Some(Self::VanillaDome),
            3 => Some(Self::ForestOfIllusion),
            4 => Some(Self::ValleyOfBowser),
            5 => Some(Self::SpecialWorld),
            6 => Some(Self::StarWorld),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OverworldLayer {
    pub tiles: Vec<u16>,
    pub width: usize,
    pub height: usize,
}

impl OverworldLayer {
    /// Constructs a rectangular layer from row-major tiles.
    ///
    /// # Errors
    ///
    /// Returns the tile vector if its length does not match the dimensions.
    pub fn new(width: usize, height: usize, tiles: Vec<u16>) -> Result<Self, Vec<u16>> {
        if width.checked_mul(height) == Some(tiles.len()) {
            Ok(Self {
                tiles,
                width,
                height,
            })
        } else {
            Err(tiles)
        }
    }

    /// Decodes a rectangular little-endian tile layer.
    ///
    /// # Errors
    ///
    /// Returns the input bytes when their length does not match the dimensions.
    pub fn decode_le(width: usize, height: usize, bytes: &[u8]) -> Result<Self, Vec<u8>> {
        let Some(tile_count) = width.checked_mul(height) else {
            return Err(bytes.to_vec());
        };
        if tile_count.checked_mul(2) != Some(bytes.len()) {
            return Err(bytes.to_vec());
        }
        Self::new(
            width,
            height,
            bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect(),
        )
        .map_err(|_| bytes.to_vec())
    }

    /// Encodes a rectangular layer after validating its public dimensions and tile vector.
    ///
    /// # Errors
    ///
    /// Returns [`OverworldLayerEncodingError`] for dimension overflow or a mismatched tile count.
    pub fn encode_le(&self) -> Result<Vec<u8>, OverworldLayerEncodingError> {
        let encoded_len = validated_layer_encoded_len(self.width, self.height, self.tiles.len())?;
        let mut encoded = Vec::with_capacity(encoded_len);
        for tile in &self.tiles {
            encoded.extend_from_slice(&tile.to_le_bytes());
        }
        Ok(encoded)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverworldLayerEncodingError {
    DimensionOverflow {
        width: usize,
        height: usize,
    },
    Shape {
        width: usize,
        height: usize,
        tiles: usize,
    },
    EncodedLengthOverflow {
        tiles: usize,
    },
}

impl std::fmt::Display for OverworldLayerEncodingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid overworld layer encoding: {self:?}")
    }
}

impl std::error::Error for OverworldLayerEncodingError {}

fn validated_layer_encoded_len(
    width: usize,
    height: usize,
    tiles: usize,
) -> Result<usize, OverworldLayerEncodingError> {
    let expected_tiles = width
        .checked_mul(height)
        .ok_or(OverworldLayerEncodingError::DimensionOverflow { width, height })?;
    if tiles != expected_tiles {
        return Err(OverworldLayerEncodingError::Shape {
            width,
            height,
            tiles,
        });
    }
    expected_tiles
        .checked_mul(2)
        .ok_or(OverworldLayerEncodingError::EncodedLengthOverflow {
            tiles: expected_tiles,
        })
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Overworld {
    pub layer1: OverworldLayer,
    pub layer2: OverworldLayer,
    pub events: Vec<EventTileChange>,
    pub endpoints: Vec<OverworldEndpoint>,
    pub paths: OverworldPathGraph,
    pub metadata: OverworldMetadata,
    pub sprites: Vec<OverworldSprite>,
    pub messages: Vec<OverworldMessage>,
    pub palettes: Vec<Palette>,
    pub animations: ExAnimationSet,
    pub unknown_extensions: Vec<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_layer_round_trips() {
        let bytes = [1, 0, 2, 0, 3, 0, 4, 0];
        let layer = OverworldLayer::decode_le(2, 2, &bytes).unwrap();
        assert_eq!(layer.tiles, [1, 2, 3, 4]);
        assert_eq!(layer.encode_le().unwrap(), bytes);
        assert!(OverworldLayer::decode_le(3, 2, &bytes).is_err());
    }

    #[test]
    fn encoding_rejects_public_shape_mismatch_and_dimension_overflow() {
        assert_eq!(
            OverworldLayer {
                tiles: vec![1],
                width: 2,
                height: 1,
            }
            .encode_le(),
            Err(OverworldLayerEncodingError::Shape {
                width: 2,
                height: 1,
                tiles: 1,
            })
        );
        assert_eq!(
            OverworldLayer {
                tiles: Vec::new(),
                width: usize::MAX,
                height: 2,
            }
            .encode_le(),
            Err(OverworldLayerEncodingError::DimensionOverflow {
                width: usize::MAX,
                height: 2,
            })
        );
        let tiles = usize::MAX / 2 + 1;
        assert_eq!(
            validated_layer_encoded_len(tiles, 1, tiles),
            Err(OverworldLayerEncodingError::EncodedLengthOverflow { tiles })
        );
    }
}
