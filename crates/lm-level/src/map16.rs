use crate::{BinaryError, ByteCursor};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Subtile(pub u16);

impl Subtile {
    #[must_use]
    pub const fn tile_number(self) -> u16 {
        self.0 & 0x03ff
    }

    #[must_use]
    pub const fn palette(self) -> u8 {
        ((self.0 >> 10) & 7) as u8
    }

    #[must_use]
    pub const fn priority(self) -> bool {
        self.0 & 0x2000 != 0
    }

    #[must_use]
    pub const fn x_flip(self) -> bool {
        self.0 & 0x4000 != 0
    }

    #[must_use]
    pub const fn y_flip(self) -> bool {
        self.0 & 0x8000 != 0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Map16Tile {
    pub top_left: Subtile,
    pub top_right: Subtile,
    pub bottom_left: Subtile,
    pub bottom_right: Subtile,
    pub acts_like: u16,
}

impl Map16Tile {
    pub const GRAPHICS_LEN: usize = 8;

    /// Decodes one eight-byte Map16 graphics definition plus its acts-like value.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError`] when the graphics definition is truncated.
    pub fn decode(graphics: &[u8], acts_like: u16) -> Result<Self, BinaryError> {
        let mut cursor = ByteCursor::new(graphics);
        Ok(Self {
            top_left: Subtile(cursor.u16_le()?),
            top_right: Subtile(cursor.u16_le()?),
            bottom_left: Subtile(cursor.u16_le()?),
            bottom_right: Subtile(cursor.u16_le()?),
            acts_like,
        })
    }

    #[must_use]
    pub fn encode_graphics(self) -> [u8; Self::GRAPHICS_LEN] {
        let words = [
            self.top_left.0,
            self.top_right.0,
            self.bottom_left.0,
            self.bottom_right.0,
        ];
        let mut result = [0; Self::GRAPHICS_LEN];
        for (target, word) in result.chunks_exact_mut(2).zip(words) {
            target.copy_from_slice(&word.to_le_bytes());
        }
        result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16Page {
    pub tiles: Vec<Map16Tile>,
}

impl Map16Page {
    pub const TILE_COUNT: usize = 256;

    /// Constructs one complete 16×16-tile page.
    ///
    /// # Errors
    ///
    /// Returns the supplied vector unchanged unless it contains exactly 256 tiles.
    pub fn new(tiles: Vec<Map16Tile>) -> Result<Self, Vec<Map16Tile>> {
        if tiles.len() == Self::TILE_COUNT {
            Ok(Self { tiles })
        } else {
            Err(tiles)
        }
    }

    /// Decodes one 0x800-byte graphics page and 0x200-byte acts-like page.
    ///
    /// # Errors
    ///
    /// Returns [`BinaryError`] when either input is truncated.
    pub fn decode(graphics: &[u8], acts_like: &[u8]) -> Result<Self, BinaryError> {
        let mut tiles = Vec::with_capacity(Self::TILE_COUNT);
        let mut acts = ByteCursor::new(acts_like);
        for index in 0..Self::TILE_COUNT {
            let start = index * Map16Tile::GRAPHICS_LEN;
            let bytes = graphics.get(start..start + Map16Tile::GRAPHICS_LEN).ok_or(
                BinaryError::UnexpectedEnd {
                    offset: start,
                    needed: Map16Tile::GRAPHICS_LEN,
                },
            )?;
            tiles.push(Map16Tile::decode(bytes, acts.u16_le()?)?);
        }
        Ok(Self { tiles })
    }

    /// Encodes exactly one canonical 256-tile page.
    ///
    /// # Errors
    ///
    /// Returns [`Map16PageEncodingError`] unless the public page has exactly 256 tiles.
    pub fn encode(&self) -> Result<(Vec<u8>, Vec<u8>), Map16PageEncodingError> {
        if self.tiles.len() != Self::TILE_COUNT {
            return Err(Map16PageEncodingError {
                tiles: self.tiles.len(),
            });
        }
        let mut graphics = Vec::with_capacity(Self::TILE_COUNT * Map16Tile::GRAPHICS_LEN);
        let mut acts_like = Vec::with_capacity(Self::TILE_COUNT * 2);
        for tile in &self.tiles {
            graphics.extend_from_slice(&tile.encode_graphics());
            acts_like.extend_from_slice(&tile.acts_like.to_le_bytes());
        }
        Ok((graphics, acts_like))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Map16PageEncodingError {
    pub tiles: usize,
}

impl std::fmt::Display for Map16PageEncodingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Map16 page must contain 256 tiles: {self:?}")
    }
}

impl std::error::Error for Map16PageEncodingError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map16_tile_round_trip() {
        let bytes = [1, 0, 2, 4, 3, 8, 4, 16];
        let tile = Map16Tile::decode(&bytes, 0x123).unwrap();
        assert_eq!(tile.encode_graphics(), bytes);
        assert_eq!(tile.acts_like, 0x123);
    }

    #[test]
    fn page_round_trip() {
        let page = Map16Page::new(vec![Map16Tile::default(); 256]).unwrap();
        let (graphics, acts_like) = page.encode().unwrap();
        assert_eq!(Map16Page::decode(&graphics, &acts_like).unwrap(), page);
    }

    #[test]
    fn malformed_public_page_cannot_encode() {
        assert_eq!(
            Map16Page { tiles: vec![] }.encode(),
            Err(Map16PageEncodingError { tiles: 0 })
        );
    }
}
