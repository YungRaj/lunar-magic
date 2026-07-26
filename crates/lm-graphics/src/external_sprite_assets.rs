use crate::{Bgr555, GraphicsFile4bpp, GraphicsFileError, IndexedTile, Rgb8};
use std::array;
use std::fmt;

pub const EXTERNAL_SPRITE_GRAPHICS_SLOTS: usize = 8;
pub const EXTERNAL_SPRITE_GRAPHICS_SLOT_MAX_BYTES: usize = 0x8000;
pub const EXTERNAL_SPRITE_GRAPHICS_BASE_TILE: u16 = 0x2000;
pub const EXTERNAL_SPRITE_GRAPHICS_TILES_PER_SLOT: usize = 0x400;
pub const EXTERNAL_SPRITE_PALETTE_ROWS: usize = 0x400;
pub const EXTERNAL_SPRITE_PALETTE_COLORS: usize = EXTERNAL_SPRITE_PALETTE_ROWS * 16;
pub const EXTERNAL_SPRITE_PALETTE_SNES_MAX_BYTES: usize = EXTERNAL_SPRITE_PALETTE_COLORS * 2;
pub const EXTERNAL_SPRITE_PALETTE_RGB_MAX_BYTES: usize = EXTERNAL_SPRITE_PALETTE_COLORS * 3;

/// Bounded decoded assets from Lunar Magic's sibling `ExternalGraphics` directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalSpriteAssets {
    graphics: [Option<GraphicsFile4bpp>; EXTERNAL_SPRITE_GRAPHICS_SLOTS],
    palette: Option<Vec<Rgb8>>,
}

impl Default for ExternalSpriteAssets {
    fn default() -> Self {
        Self {
            graphics: array::from_fn(|_| None),
            palette: None,
        }
    }
}

impl ExternalSpriteAssets {
    /// Decodes and installs one `ExSpriteGFXxx.bin` slot.
    ///
    /// # Errors
    ///
    /// Rejects invalid slot numbers, empty/oversized files, and partial 4-bpp tiles without
    /// changing the asset set.
    pub fn set_graphics_slot(
        &mut self,
        slot: usize,
        bytes: &[u8],
    ) -> Result<(), ExternalSpriteAssetsError> {
        if slot >= EXTERNAL_SPRITE_GRAPHICS_SLOTS {
            return Err(ExternalSpriteAssetsError::GraphicsSlot(slot));
        }
        if bytes.is_empty() || bytes.len() > EXTERNAL_SPRITE_GRAPHICS_SLOT_MAX_BYTES {
            return Err(ExternalSpriteAssetsError::GraphicsLength {
                slot,
                actual: bytes.len(),
            });
        }
        let decoded =
            GraphicsFile4bpp::decode(bytes).map_err(ExternalSpriteAssetsError::Graphics)?;
        self.graphics[slot] = Some(decoded);
        Ok(())
    }

    /// Decodes a nonempty prefix of Lunar Magic's raw little-endian SNES `.mw3` palette.
    ///
    /// # Errors
    ///
    /// Rejects odd, empty, or oversized inputs without replacing an existing palette.
    pub fn set_snes_palette(&mut self, bytes: &[u8]) -> Result<(), ExternalSpriteAssetsError> {
        if bytes.is_empty()
            || bytes.len() > EXTERNAL_SPRITE_PALETTE_SNES_MAX_BYTES
            || bytes.len() % 2 != 0
        {
            return Err(ExternalSpriteAssetsError::SnesPaletteLength(bytes.len()));
        }
        let colors = bytes
            .chunks_exact(2)
            .map(|word| Bgr555(u16::from_le_bytes([word[0], word[1]])).to_rgb8())
            .collect();
        self.palette = Some(colors);
        Ok(())
    }

    /// Decodes a nonempty prefix of Lunar Magic's packed RGB24 `.pal` palette.
    ///
    /// # Errors
    ///
    /// Rejects partial triplets, empty files, and inputs above the recovered 0xC000-byte cap
    /// without replacing an existing palette.
    pub fn set_rgb_palette(&mut self, bytes: &[u8]) -> Result<(), ExternalSpriteAssetsError> {
        if bytes.is_empty()
            || bytes.len() > EXTERNAL_SPRITE_PALETTE_RGB_MAX_BYTES
            || bytes.len() % 3 != 0
        {
            return Err(ExternalSpriteAssetsError::RgbPaletteLength(bytes.len()));
        }
        self.palette = Some(
            bytes
                .chunks_exact(3)
                .map(|rgb| Rgb8 {
                    red: rgb[0],
                    green: rgb[1],
                    blue: rgb[2],
                })
                .collect(),
        );
        Ok(())
    }

    #[must_use]
    pub fn graphics_tile(&self, global_tile: u16) -> Option<&IndexedTile> {
        let relative = global_tile.checked_sub(EXTERNAL_SPRITE_GRAPHICS_BASE_TILE)?;
        let relative = usize::from(relative);
        let slot = relative / EXTERNAL_SPRITE_GRAPHICS_TILES_PER_SLOT;
        let tile = relative % EXTERNAL_SPRITE_GRAPHICS_TILES_PER_SLOT;
        self.graphics.get(slot)?.as_ref()?.tiles.get(tile)
    }

    /// Resolves one custom-palette color using Lunar Magic's base-row plus subtile-row rule.
    #[must_use]
    pub fn palette_color(
        &self,
        palette_source: u16,
        subtile_palette: u8,
        color: u8,
    ) -> Option<Rgb8> {
        if palette_source >= u16::try_from(EXTERNAL_SPRITE_PALETTE_ROWS).ok()?
            || subtile_palette > 7
            || color > 15
        {
            return None;
        }
        let row = usize::from(palette_source).checked_add(usize::from(subtile_palette))?;
        let index = row.checked_mul(16)?.checked_add(usize::from(color))?;
        self.palette.as_ref()?.get(index).copied()
    }

    #[must_use]
    pub fn has_graphics_slot(&self, slot: usize) -> bool {
        self.graphics.get(slot).is_some_and(Option::is_some)
    }

    #[must_use]
    pub fn has_palette(&self) -> bool {
        self.palette.is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalSpriteAssetsError {
    GraphicsSlot(usize),
    GraphicsLength { slot: usize, actual: usize },
    Graphics(GraphicsFileError),
    SnesPaletteLength(usize),
    RgbPaletteLength(usize),
}

impl fmt::Display for ExternalSpriteAssetsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid external sprite assets: {self:?}")
    }
}

impl std::error::Error for ExternalSpriteAssetsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rgb8, encode_4bpp_tile};

    #[test]
    fn graphics_slots_map_exactly_to_global_2000_through_3fff() {
        let mut assets = ExternalSpriteAssets::default();
        let tile = IndexedTile::new([3; IndexedTile::PIXEL_COUNT]);
        let encoded = encode_4bpp_tile(&tile).unwrap();
        assets.set_graphics_slot(0, &encoded).unwrap();
        assets.set_graphics_slot(7, &encoded).unwrap();
        assert_eq!(assets.graphics_tile(0x2000), Some(&tile));
        assert_eq!(assets.graphics_tile(0x3c00), Some(&tile));
        assert_eq!(assets.graphics_tile(0x1fff), None);
        assert_eq!(assets.graphics_tile(0x4000), None);
        assert_eq!(assets.graphics_tile(0x2400), None);
    }

    #[test]
    fn both_palette_encodings_use_base_plus_subtile_row() {
        let mut snes = vec![0; 0x22 * 2];
        let expected = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 255,
        });
        snes[0x21 * 2..0x21 * 2 + 2].copy_from_slice(&expected.0.to_le_bytes());
        let mut assets = ExternalSpriteAssets::default();
        assets.set_snes_palette(&snes).unwrap();
        assert_eq!(assets.palette_color(1, 1, 1), Some(expected.to_rgb8()));

        let mut rgb = vec![0; 0x22 * 3];
        rgb[0x21 * 3..0x21 * 3 + 3].copy_from_slice(&[1, 2, 3]);
        assets.set_rgb_palette(&rgb).unwrap();
        assert_eq!(
            assets.palette_color(1, 1, 1),
            Some(Rgb8 {
                red: 1,
                green: 2,
                blue: 3,
            })
        );
    }

    #[test]
    fn malformed_replacements_leave_existing_assets_intact() {
        let mut assets = ExternalSpriteAssets::default();
        assets.set_graphics_slot(0, &[0; 32]).unwrap();
        assets.set_rgb_palette(&[1, 2, 3]).unwrap();
        let original = assets.clone();
        for bad in [Vec::new(), vec![0; 31], vec![0; 0x8001]] {
            assert!(assets.set_graphics_slot(0, &bad).is_err());
            assert_eq!(assets, original);
        }
        assert!(assets.set_snes_palette(&[0]).is_err());
        assert!(assets.set_rgb_palette(&[0, 1]).is_err());
        assert_eq!(assets, original);
    }
}
