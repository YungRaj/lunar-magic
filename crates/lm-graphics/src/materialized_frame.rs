use crate::{Bgr555, GraphicsFile4bpp, IndexedTile, Palette};
use std::fmt;

mod application;
mod validation;

use validation::{encoded_len, validate_counts};

/// One exact tile replacement resolved by an animation provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedTileOverride {
    pub tile_index: u32,
    pub tile: IndexedTile,
}

/// One exact palette-color replacement resolved by an animation provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaterializedPaletteOverride {
    pub color_index: u32,
    pub color: Bgr555,
}

/// Provider-neutral graphics and palette state for one animation tick.
///
/// This is deliberately the result of interpreting `ExAnimation`, not another
/// interpretation of Lunar Magic's undocumented transfer types. An oracle or
/// a future verified interpreter can produce this frame and every renderer can
/// consume it without duplicating those rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaterializedAnimationFrame {
    pub tick: u32,
    pub tile_overrides: Vec<MaterializedTileOverride>,
    pub palette_overrides: Vec<MaterializedPaletteOverride>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MaterializedFrameError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    NonZeroReserved,
    TooManyTileOverrides(usize),
    TooManyPaletteOverrides(usize),
    LengthOverflow,
    WrongLength { expected: usize, actual: usize },
    DuplicateTile(u32),
    DuplicateColor(u32),
    PixelOutOfRange { tile_index: u32, pixel: u8 },
    ColorValueOutOfRange { color_index: u32, value: u16 },
    TileTargetOutOfRange { index: u32, len: usize },
    ColorTargetOutOfRange { index: u32, len: usize },
}

impl fmt::Display for MaterializedFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid materialized animation frame: {self:?}")
    }
}

impl std::error::Error for MaterializedFrameError {}

impl MaterializedAnimationFrame {
    pub const MAGIC: [u8; 8] = *b"LMANFRM\0";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 24;
    pub const MAX_TILE_OVERRIDES: usize = 0x1_0000;
    pub const MAX_PALETTE_OVERRIDES: usize = 0x1_0000;
    const TILE_ENTRY_LEN: usize = 4 + IndexedTile::PIXEL_COUNT;
    const PALETTE_ENTRY_LEN: usize = 8;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN
        + Self::MAX_TILE_OVERRIDES * Self::TILE_ENTRY_LEN
        + Self::MAX_PALETTE_OVERRIDES * Self::PALETTE_ENTRY_LEN;

    /// Serializes a canonical frame, sorting unique targets by absolute index.
    ///
    /// # Errors
    ///
    /// Returns [`MaterializedFrameError`] for duplicate targets, invalid 4bpp
    /// pixels/BGR555 words, count limits, or arithmetic overflow.
    pub fn encode(&self) -> Result<Vec<u8>, MaterializedFrameError> {
        self.validate_values()?;
        let expected = encoded_len(self.tile_overrides.len(), self.palette_overrides.len())?;
        let mut tiles: Vec<_> = self.tile_overrides.iter().collect();
        tiles.sort_unstable_by_key(|entry| entry.tile_index);
        let mut colors: Vec<_> = self.palette_overrides.iter().collect();
        colors.sort_unstable_by_key(|entry| entry.color_index);

        let mut bytes = Vec::with_capacity(expected);
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&self.tick.to_le_bytes());
        let tile_count = u32::try_from(tiles.len())
            .map_err(|_| MaterializedFrameError::TooManyTileOverrides(tiles.len()))?;
        let color_count = u32::try_from(colors.len())
            .map_err(|_| MaterializedFrameError::TooManyPaletteOverrides(colors.len()))?;
        bytes.extend_from_slice(&tile_count.to_le_bytes());
        bytes.extend_from_slice(&color_count.to_le_bytes());
        for entry in tiles {
            bytes.extend_from_slice(&entry.tile_index.to_le_bytes());
            bytes.extend_from_slice(entry.tile.pixels());
        }
        for entry in colors {
            bytes.extend_from_slice(&entry.color_index.to_le_bytes());
            bytes.extend_from_slice(&entry.color.0.to_le_bytes());
            bytes.extend_from_slice(&0_u16.to_le_bytes());
        }
        Ok(bytes)
    }

    /// Decodes exact provider-resolved animation state with strict framing.
    ///
    /// # Errors
    ///
    /// Returns [`MaterializedFrameError`] for invalid headers, lengths, values,
    /// counts, or duplicate targets.
    pub fn decode(bytes: &[u8]) -> Result<Self, MaterializedFrameError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(MaterializedFrameError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(MaterializedFrameError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(MaterializedFrameError::UnsupportedVersion(version));
        }
        if header[10] != 0 || header[11] != 0 {
            return Err(MaterializedFrameError::NonZeroReserved);
        }
        let tick = read_u32(header, 12);
        let tile_count = read_count(header, 16)?;
        let palette_count = read_count(header, 20)?;
        validate_counts(tile_count, palette_count)?;
        let expected = encoded_len(tile_count, palette_count)?;
        if bytes.len() != expected {
            return Err(MaterializedFrameError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }

        let mut offset = Self::HEADER_LEN;
        let mut tile_overrides = Vec::with_capacity(tile_count);
        for _ in 0..tile_count {
            let tile_index = read_u32(bytes, offset);
            offset += 4;
            let mut pixels = [0; IndexedTile::PIXEL_COUNT];
            pixels.copy_from_slice(&bytes[offset..offset + IndexedTile::PIXEL_COUNT]);
            offset += IndexedTile::PIXEL_COUNT;
            tile_overrides.push(MaterializedTileOverride {
                tile_index,
                tile: IndexedTile::new(pixels),
            });
        }
        let mut palette_overrides = Vec::with_capacity(palette_count);
        for _ in 0..palette_count {
            let color_index = read_u32(bytes, offset);
            let color = u16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]);
            let reserved = u16::from_le_bytes([bytes[offset + 6], bytes[offset + 7]]);
            if reserved != 0 {
                return Err(MaterializedFrameError::NonZeroReserved);
            }
            offset += Self::PALETTE_ENTRY_LEN;
            palette_overrides.push(MaterializedPaletteOverride {
                color_index,
                color: Bgr555(color),
            });
        }
        let frame = Self {
            tick,
            tile_overrides,
            palette_overrides,
        };
        frame.validate_values()?;
        Ok(frame)
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("checked frame length"),
    )
}

fn read_count(bytes: &[u8], offset: usize) -> Result<usize, MaterializedFrameError> {
    usize::try_from(read_u32(bytes, offset)).map_err(|_| MaterializedFrameError::LengthOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> MaterializedAnimationFrame {
        MaterializedAnimationFrame {
            tick: 91,
            tile_overrides: vec![
                MaterializedTileOverride {
                    tile_index: 3,
                    tile: IndexedTile::new([3; IndexedTile::PIXEL_COUNT]),
                },
                MaterializedTileOverride {
                    tile_index: 1,
                    tile: IndexedTile::new([1; IndexedTile::PIXEL_COUNT]),
                },
            ],
            palette_overrides: vec![
                MaterializedPaletteOverride {
                    color_index: 2,
                    color: Bgr555(0x4210),
                },
                MaterializedPaletteOverride {
                    color_index: 0,
                    color: Bgr555(0x001f),
                },
            ],
        }
    }

    #[test]
    fn exact_frame_round_trips_to_canonical_order() {
        let bytes = frame().encode().unwrap();
        let decoded = MaterializedAnimationFrame::decode(&bytes).unwrap();
        assert_eq!(decoded.tick, 91);
        assert_eq!(
            decoded
                .tile_overrides
                .iter()
                .map(|entry| entry.tile_index)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(
            decoded
                .palette_overrides
                .iter()
                .map(|entry| entry.color_index)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(decoded.encode().unwrap(), bytes);
    }

    #[test]
    fn framing_reserved_values_and_counts_are_checked() {
        let bytes = frame().encode().unwrap();
        assert_eq!(
            MaterializedAnimationFrame::decode(&bytes[..23]),
            Err(MaterializedFrameError::Truncated)
        );
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            MaterializedAnimationFrame::decode(&trailing),
            Err(MaterializedFrameError::WrongLength { .. })
        ));
        let mut reserved = bytes.clone();
        reserved[10] = 1;
        assert_eq!(
            MaterializedAnimationFrame::decode(&reserved),
            Err(MaterializedFrameError::NonZeroReserved)
        );
        let mut count = bytes;
        count[16..20].copy_from_slice(&0x1_0001_u32.to_le_bytes());
        assert_eq!(
            MaterializedAnimationFrame::decode(&count),
            Err(MaterializedFrameError::TooManyTileOverrides(0x1_0001))
        );
    }

    #[test]
    fn duplicate_and_non_snes_values_are_rejected() {
        let mut duplicate = frame();
        duplicate.tile_overrides[1].tile_index = 3;
        assert_eq!(
            duplicate.encode(),
            Err(MaterializedFrameError::DuplicateTile(3))
        );
        let invalid_pixel = MaterializedAnimationFrame {
            tick: 0,
            tile_overrides: vec![MaterializedTileOverride {
                tile_index: 0,
                tile: IndexedTile::new([16; IndexedTile::PIXEL_COUNT]),
            }],
            palette_overrides: Vec::new(),
        };
        assert!(matches!(
            invalid_pixel.encode(),
            Err(MaterializedFrameError::PixelOutOfRange { .. })
        ));
        let mut invalid_color = frame();
        invalid_color.palette_overrides[0].color = Bgr555(0x8000);
        assert!(matches!(
            invalid_color.encode(),
            Err(MaterializedFrameError::ColorValueOutOfRange { .. })
        ));
    }

    #[test]
    fn application_is_exact_and_validates_all_targets_first() {
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; 64]); 4],
        };
        let palette = Palette {
            colors: vec![Bgr555(0); 4],
        };
        let (animated_graphics, animated_palette) = frame().apply(&graphics, &palette).unwrap();
        assert_eq!(animated_graphics.tiles[1].pixels(), &[1; 64]);
        assert_eq!(animated_graphics.tiles[3].pixels(), &[3; 64]);
        assert_eq!(animated_palette.colors[0], Bgr555(0x001f));
        assert_eq!(graphics.tiles[1].pixels(), &[0; 64]);
        assert_eq!(palette.colors[0], Bgr555(0));

        let mut invalid = frame();
        invalid.palette_overrides.push(MaterializedPaletteOverride {
            color_index: 9,
            color: Bgr555(1),
        });
        assert_eq!(
            invalid.apply(&graphics, &palette),
            Err(MaterializedFrameError::ColorTargetOutOfRange { index: 9, len: 4 })
        );
        assert_eq!(graphics.tiles[3].pixels(), &[0; 64]);
        assert_eq!(palette.colors[2], Bgr555(0));
    }
}
