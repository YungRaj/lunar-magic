#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedTile {
    pixels: [u8; Self::PIXEL_COUNT],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileEditError {
    CoordinateOutOfRange { x: usize, y: usize },
    ColorOutOfRange(u8),
}

impl std::fmt::Display for TileEditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid indexed-tile edit: {self:?}")
    }
}

impl std::error::Error for TileEditError {}

impl IndexedTile {
    pub const WIDTH: usize = 8;
    pub const HEIGHT: usize = 8;
    pub const PIXEL_COUNT: usize = Self::WIDTH * Self::HEIGHT;

    #[must_use]
    pub const fn new(pixels: [u8; Self::PIXEL_COUNT]) -> Self {
        Self { pixels }
    }

    #[must_use]
    pub const fn pixels(&self) -> &[u8; Self::PIXEL_COUNT] {
        &self.pixels
    }

    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> Option<u8> {
        (x < Self::WIDTH && y < Self::HEIGHT).then(|| self.pixels[y * Self::WIDTH + x])
    }

    /// Changes one 4bpp pixel.
    ///
    /// # Errors
    ///
    /// Returns [`TileEditError`] for coordinates outside 8×8 or a color index above 15.
    pub fn set_pixel(&mut self, x: usize, y: usize, color: u8) -> Result<(), TileEditError> {
        if x >= Self::WIDTH || y >= Self::HEIGHT {
            return Err(TileEditError::CoordinateOutOfRange { x, y });
        }
        if color > 0x0f {
            return Err(TileEditError::ColorOutOfRange(color));
        }
        self.pixels[y * Self::WIDTH + x] = color;
        Ok(())
    }

    #[must_use]
    pub fn flipped(&self, horizontal: bool, vertical: bool) -> Self {
        let mut pixels = [0; Self::PIXEL_COUNT];
        for y in 0..Self::HEIGHT {
            for x in 0..Self::WIDTH {
                let source_x = if horizontal { Self::WIDTH - 1 - x } else { x };
                let source_y = if vertical { Self::HEIGHT - 1 - y } else { y };
                pixels[y * Self::WIDTH + x] = self.pixels[source_y * Self::WIDTH + source_x];
            }
        }
        Self { pixels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_edits_are_bounded_and_four_bit() {
        let mut tile = IndexedTile::new([0; IndexedTile::PIXEL_COUNT]);
        tile.set_pixel(7, 7, 15).unwrap();
        assert_eq!(tile.pixel(7, 7), Some(15));
        let original = tile.clone();
        assert!(matches!(
            tile.set_pixel(8, 0, 1),
            Err(TileEditError::CoordinateOutOfRange { x: 8, y: 0 })
        ));
        assert!(matches!(
            tile.set_pixel(0, 0, 16),
            Err(TileEditError::ColorOutOfRange(16))
        ));
        assert_eq!(tile, original);
        assert_eq!(tile.pixel(8, 0), None);
    }
}
