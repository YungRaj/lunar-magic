#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedTile {
    pixels: [u8; Self::PIXEL_COUNT],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileEditError {
    CoordinateOutOfRange { x: usize, y: usize },
    ColorOutOfRange(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileShift {
    Left,
    Right,
    Up,
    Down,
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

    /// Returns the tile rotated 90 degrees clockwise.
    #[must_use]
    pub fn rotated_clockwise(&self) -> Self {
        let mut pixels = [0; Self::PIXEL_COUNT];
        for y in 0..Self::HEIGHT {
            for x in 0..Self::WIDTH {
                let source_x = y;
                let source_y = Self::HEIGHT - 1 - x;
                pixels[y * Self::WIDTH + x] = self.pixels[source_y * Self::WIDTH + source_x];
            }
        }
        Self { pixels }
    }

    #[must_use]
    pub fn shifted_wrapping(&self, direction: TileShift) -> Self {
        let mut pixels = [0; Self::PIXEL_COUNT];
        for y in 0..Self::HEIGHT {
            for x in 0..Self::WIDTH {
                let (source_x, source_y) = match direction {
                    TileShift::Left => ((x + 1) % Self::WIDTH, y),
                    TileShift::Right => ((x + Self::WIDTH - 1) % Self::WIDTH, y),
                    TileShift::Up => (x, (y + 1) % Self::HEIGHT),
                    TileShift::Down => (x, (y + Self::HEIGHT - 1) % Self::HEIGHT),
                };
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

    #[test]
    fn wrapping_shifts_move_one_pixel_and_are_exactly_reversible() {
        let tile = IndexedTile::new(std::array::from_fn(|index| index.to_le_bytes()[0] & 0x0f));
        let left = tile.shifted_wrapping(TileShift::Left);
        assert_eq!(left.pixel(0, 0), tile.pixel(1, 0));
        assert_eq!(left.pixel(7, 0), tile.pixel(0, 0));
        assert_eq!(left.shifted_wrapping(TileShift::Right), tile);

        let up = tile.shifted_wrapping(TileShift::Up);
        assert_eq!(up.pixel(0, 0), tile.pixel(0, 1));
        assert_eq!(up.pixel(0, 7), tile.pixel(0, 0));
        assert_eq!(up.shifted_wrapping(TileShift::Down), tile);

        for direction in [
            TileShift::Left,
            TileShift::Right,
            TileShift::Up,
            TileShift::Down,
        ] {
            let cycled = (0..IndexedTile::WIDTH)
                .fold(tile.clone(), |tile, _| tile.shifted_wrapping(direction));
            assert_eq!(cycled, tile);
        }
    }

    #[test]
    fn clockwise_rotation_matches_the_native_editor_and_cycles_in_four_steps() {
        let mut pixels = [0; IndexedTile::PIXEL_COUNT];
        pixels[0] = 1;
        pixels[7] = 2;
        pixels[7 * IndexedTile::WIDTH] = 3;
        let tile = IndexedTile::new(pixels);
        let rotated = tile.rotated_clockwise();

        assert_eq!(rotated.pixel(7, 0), Some(1));
        assert_eq!(rotated.pixel(7, 7), Some(2));
        assert_eq!(rotated.pixel(0, 0), Some(3));
        assert_eq!(
            (0..4).fold(tile.clone(), |tile, _| tile.rotated_clockwise()),
            tile
        );
    }
}
