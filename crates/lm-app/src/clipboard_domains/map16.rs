use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_level::Map16Tile;
use std::fmt;

/// Lunar Magic's registered `Lunar Magic 16x16 Tiles` rectangular clipboard payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMap16Clipboard {
    pub selected_count: u32,
    pub width: u32,
    pub height: u32,
    pub source_index: u32,
    pub alternate_word_order: bool,
    pub tiles: Vec<Map16Tile>,
    pub source_indices: Vec<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeMap16ClipboardError {
    Truncated,
    Shape {
        selected: u32,
        width: u32,
        height: u32,
        tiles: usize,
        source_indices: usize,
    },
    SourceRectangle {
        source_index: u32,
        width: u32,
        height: u32,
    },
    Header,
    SectionBounds,
    Flags(u32),
    Tile(usize),
}

impl fmt::Display for NativeMap16ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid Lunar Magic 16x16 Tiles clipboard payload: {self:?}"
        )
    }
}

impl std::error::Error for NativeMap16ClipboardError {}

impl NativeMap16Clipboard {
    pub const FORMAT_NAME: &'static str = "Lunar Magic 16x16 Tiles";
    pub const HEADER_LEN: usize = 0xa0;
    pub const MAX_WIDTH: u32 = 0x10;
    pub const MAX_TILE_INDEX: u32 = 0x13d00;

    /// Builds Lunar Magic's canonical full rectangular selection from one row-major tile slice.
    ///
    /// # Errors
    ///
    /// Rejects empty/mismatched rectangles, page-row wrapping, or source namespace overflow.
    pub fn from_rectangle(
        source_index: u32,
        width: u32,
        height: u32,
        tiles: Vec<Map16Tile>,
    ) -> Result<Self, NativeMap16ClipboardError> {
        let area = rectangle_area(width, height).ok_or(NativeMap16ClipboardError::Shape {
            selected: 0,
            width,
            height,
            tiles: tiles.len(),
            source_indices: 0,
        })?;
        if tiles.len() != area {
            return Err(NativeMap16ClipboardError::Shape {
                selected: u32::try_from(area).unwrap_or(u32::MAX),
                width,
                height,
                tiles: tiles.len(),
                source_indices: 0,
            });
        }
        let source_indices = rectangle_source_indices(source_index, width, height)?;
        Ok(Self {
            selected_count: u32::try_from(area).expect("native Map16 area is bounded"),
            width,
            height,
            source_index,
            alternate_word_order: false,
            tiles,
            source_indices,
        })
    }

    /// Encodes the exact canonical 0xA0-byte header and three native sections.
    ///
    /// # Errors
    ///
    /// Rejects malformed public shapes, unsupported flags, or encoded-length overflow.
    pub fn encode(&self) -> Result<Vec<u8>, NativeMap16ClipboardError> {
        let area = self.validate_shape()?;
        let definitions_len = area
            .checked_mul(Map16Tile::GRAPHICS_LEN)
            .ok_or(NativeMap16ClipboardError::SectionBounds)?;
        let behavior_len = area
            .checked_mul(2)
            .ok_or(NativeMap16ClipboardError::SectionBounds)?;
        let index_len = area
            .checked_mul(4)
            .ok_or(NativeMap16ClipboardError::SectionBounds)?;
        let definition_offset = Self::HEADER_LEN;
        let behavior_offset = definition_offset
            .checked_add(definitions_len)
            .ok_or(NativeMap16ClipboardError::SectionBounds)?;
        let index_offset = behavior_offset
            .checked_add(behavior_len)
            .ok_or(NativeMap16ClipboardError::SectionBounds)?;
        let total = index_offset
            .checked_add(index_len)
            .ok_or(NativeMap16ClipboardError::SectionBounds)?;
        let mut bytes = vec![0; total];
        put_u32(&mut bytes, 0x00, definition_offset)?;
        put_u32(&mut bytes, 0x04, behavior_offset)?;
        put_u32(&mut bytes, 0x08, index_offset)?;
        bytes[0x50..0x54].copy_from_slice(&self.selected_count.to_le_bytes());
        bytes[0x54..0x58].copy_from_slice(&self.width.to_le_bytes());
        bytes[0x58..0x5c].copy_from_slice(&self.height.to_le_bytes());
        bytes[0x5c..0x60].copy_from_slice(&self.source_index.to_le_bytes());
        bytes[0x60..0x64].copy_from_slice(&u32::from(self.alternate_word_order).to_le_bytes());
        for (index, tile) in self.tiles.iter().enumerate() {
            let at = definition_offset + index * Map16Tile::GRAPHICS_LEN;
            let mut definition = tile.encode_graphics();
            if self.alternate_word_order {
                definition[2..6].rotate_left(2);
            }
            bytes[at..at + Map16Tile::GRAPHICS_LEN].copy_from_slice(&definition);
            let at = behavior_offset + index * 2;
            bytes[at..at + 2].copy_from_slice(&tile.acts_like.to_le_bytes());
            let at = index_offset + index * 4;
            bytes[at..at + 4].copy_from_slice(&self.source_indices[index].to_le_bytes());
        }
        Ok(bytes)
    }

    /// Decodes a canonical native rectangular Map16 clipboard payload.
    ///
    /// # Errors
    ///
    /// Rejects invalid header fields, noncanonical section framing, bad tile definitions, and
    /// rectangles outside Lunar Magic's bounded 16-column workspace.
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeMap16ClipboardError> {
        if bytes.len() < Self::HEADER_LEN {
            return Err(NativeMap16ClipboardError::Truncated);
        }
        let definition_offset = usize::try_from(read_u32(bytes, 0x00)?)
            .map_err(|_| NativeMap16ClipboardError::Header)?;
        let behavior_offset = usize::try_from(read_u32(bytes, 0x04)?)
            .map_err(|_| NativeMap16ClipboardError::Header)?;
        let index_offset = usize::try_from(read_u32(bytes, 0x08)?)
            .map_err(|_| NativeMap16ClipboardError::Header)?;
        let selected_count = read_u32(bytes, 0x50)?;
        let width = read_u32(bytes, 0x54)?;
        let height = read_u32(bytes, 0x58)?;
        let source_index = read_u32(bytes, 0x5c)?;
        let flags = read_u32(bytes, 0x60)?;
        if flags > 1 {
            return Err(NativeMap16ClipboardError::Flags(flags));
        }
        let area = rectangle_area(width, height).ok_or(NativeMap16ClipboardError::Shape {
            selected: selected_count,
            width,
            height,
            tiles: 0,
            source_indices: 0,
        })?;
        if selected_count == 0
            || match usize::try_from(selected_count) {
                Ok(count) => count > area,
                Err(_) => true,
            }
        {
            return Err(NativeMap16ClipboardError::Shape {
                selected: selected_count,
                width,
                height,
                tiles: area,
                source_indices: area,
            });
        }
        let definitions_len = area * Map16Tile::GRAPHICS_LEN;
        let behavior_len = area * 2;
        let index_len = area * 4;
        if definition_offset != Self::HEADER_LEN
            || behavior_offset != definition_offset + definitions_len
            || index_offset != behavior_offset + behavior_len
            || index_offset + index_len != bytes.len()
            || bytes[0x0c..0x50].iter().any(|byte| *byte != 0)
            || bytes[0x64..Self::HEADER_LEN].iter().any(|byte| *byte != 0)
        {
            return Err(NativeMap16ClipboardError::Header);
        }
        let mut tiles = Vec::with_capacity(area);
        let mut source_indices = Vec::with_capacity(area);
        for index in 0..area {
            let definition_at = definition_offset + index * Map16Tile::GRAPHICS_LEN;
            let mut definition: [u8; Map16Tile::GRAPHICS_LEN] = bytes
                [definition_at..definition_at + Map16Tile::GRAPHICS_LEN]
                .try_into()
                .expect("validated native definition range");
            if flags == 1 {
                definition[2..6].rotate_right(2);
            }
            let behavior_at = behavior_offset + index * 2;
            let acts_like = u16::from_le_bytes(
                bytes[behavior_at..behavior_at + 2]
                    .try_into()
                    .expect("validated native behavior range"),
            );
            tiles.push(
                Map16Tile::decode(&definition, acts_like)
                    .map_err(|_| NativeMap16ClipboardError::Tile(index))?,
            );
            let index_at = index_offset + index * 4;
            source_indices.push(u32::from_le_bytes(
                bytes[index_at..index_at + 4]
                    .try_into()
                    .expect("validated native index range"),
            ));
        }
        Ok(Self {
            selected_count,
            width,
            height,
            source_index,
            alternate_word_order: flags == 1,
            tiles,
            source_indices,
        })
    }

    fn validate_shape(&self) -> Result<usize, NativeMap16ClipboardError> {
        let area =
            rectangle_area(self.width, self.height).ok_or(NativeMap16ClipboardError::Shape {
                selected: self.selected_count,
                width: self.width,
                height: self.height,
                tiles: self.tiles.len(),
                source_indices: self.source_indices.len(),
            })?;
        if self.selected_count == 0
            || match usize::try_from(self.selected_count) {
                Ok(count) => count > area,
                Err(_) => true,
            }
            || self.tiles.len() != area
            || self.source_indices.len() != area
        {
            return Err(NativeMap16ClipboardError::Shape {
                selected: self.selected_count,
                width: self.width,
                height: self.height,
                tiles: self.tiles.len(),
                source_indices: self.source_indices.len(),
            });
        }
        Ok(area)
    }
}

fn rectangle_area(width: u32, height: u32) -> Option<usize> {
    if width == 0 || width > NativeMap16Clipboard::MAX_WIDTH || height == 0 {
        return None;
    }
    let area = width.checked_mul(height)?;
    if area > NativeMap16Clipboard::MAX_TILE_INDEX {
        return None;
    }
    usize::try_from(area).ok()
}

fn rectangle_source_indices(
    source_index: u32,
    width: u32,
    height: u32,
) -> Result<Vec<u32>, NativeMap16ClipboardError> {
    if source_index >= NativeMap16Clipboard::MAX_TILE_INDEX
        || source_index % NativeMap16Clipboard::MAX_WIDTH + width > NativeMap16Clipboard::MAX_WIDTH
    {
        return Err(NativeMap16ClipboardError::SourceRectangle {
            source_index,
            width,
            height,
        });
    }
    let mut indices = Vec::with_capacity(rectangle_area(width, height).unwrap_or(0));
    for row in 0..height {
        for column in 0..width {
            let index = source_index
                .checked_add(row * NativeMap16Clipboard::MAX_WIDTH)
                .and_then(|index| index.checked_add(column))
                .filter(|index| *index < NativeMap16Clipboard::MAX_TILE_INDEX)
                .ok_or(NativeMap16ClipboardError::SourceRectangle {
                    source_index,
                    width,
                    height,
                })?;
            indices.push(index);
        }
    }
    Ok(indices)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, NativeMap16ClipboardError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(NativeMap16ClipboardError::Truncated)?;
    Ok(u32::from_le_bytes(
        value.try_into().expect("validated four-byte field"),
    ))
}

fn put_u32(bytes: &mut [u8], offset: usize, value: usize) -> Result<(), NativeMap16ClipboardError> {
    let value = u32::try_from(value).map_err(|_| NativeMap16ClipboardError::SectionBounds)?;
    bytes
        .get_mut(offset..offset + 4)
        .ok_or(NativeMap16ClipboardError::SectionBounds)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

impl ClipboardPayload {
    #[must_use]
    pub fn from_map16_tiles(tiles: &[Map16Tile]) -> Self {
        let records = tiles
            .iter()
            .map(|tile| {
                let mut record = tile.encode_graphics().to_vec();
                record.extend_from_slice(&tile.acts_like.to_le_bytes());
                record
            })
            .collect();
        Self {
            kind: ClipboardKind::Map16Tiles,
            records,
        }
    }

    /// Decodes complete graphics and Acts Like fields for each Map16 tile.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or non-ten-byte records.
    pub fn to_map16_tiles(&self) -> Result<Vec<Map16Tile>, ClipboardError> {
        self.require_kind(ClipboardKind::Map16Tiles)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                if record.len() != 10 {
                    return Err(ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    });
                }
                Map16Tile::decode(&record[..8], u16::from_le_bytes([record[8], record[9]])).map_err(
                    |_| ClipboardError::InvalidRecord {
                        index,
                        length: record.len(),
                    },
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod native_tests {
    use super::*;
    use lm_level::Subtile;

    fn tile(seed: u16) -> Map16Tile {
        Map16Tile {
            top_left: Subtile(seed),
            top_right: Subtile(seed + 1),
            bottom_left: Subtile(seed + 2),
            bottom_right: Subtile(seed + 3),
            acts_like: seed + 4,
        }
    }

    #[test]
    fn native_rectangle_uses_exact_header_sections_and_row_stride() {
        let tiles: Vec<_> = (0..6).map(|index| tile(index * 0x10)).collect();
        let clipboard = NativeMap16Clipboard::from_rectangle(0x21, 3, 2, tiles.clone()).unwrap();
        assert_eq!(
            clipboard.source_indices,
            [0x21, 0x22, 0x23, 0x31, 0x32, 0x33]
        );

        let encoded = clipboard.encode().unwrap();
        assert_eq!(encoded.len(), 0xa0 + 6 * 14);
        assert_eq!(read_u32(&encoded, 0x00).unwrap(), 0xa0);
        assert_eq!(read_u32(&encoded, 0x04).unwrap(), 0xd0);
        assert_eq!(read_u32(&encoded, 0x08).unwrap(), 0xdc);
        assert_eq!(read_u32(&encoded, 0x50).unwrap(), 6);
        assert_eq!(read_u32(&encoded, 0x54).unwrap(), 3);
        assert_eq!(read_u32(&encoded, 0x58).unwrap(), 2);
        assert_eq!(read_u32(&encoded, 0x5c).unwrap(), 0x21);
        assert_eq!(read_u32(&encoded, 0x60).unwrap(), 0);
        assert_eq!(&encoded[0xa0..0xa8], &tiles[0].encode_graphics());
        assert_eq!(&encoded[0xd0..0xd2], &tiles[0].acts_like.to_le_bytes());
        assert_eq!(NativeMap16Clipboard::decode(&encoded).unwrap(), clipboard);
    }

    #[test]
    fn native_alternate_word_order_is_normalized_and_reencoded() {
        let mut clipboard =
            NativeMap16Clipboard::from_rectangle(0x40, 1, 1, vec![tile(0x100)]).unwrap();
        clipboard.alternate_word_order = true;
        let encoded = clipboard.encode().unwrap();
        let words: Vec<_> = encoded[0xa0..0xa8]
            .chunks_exact(2)
            .map(|word| u16::from_le_bytes([word[0], word[1]]))
            .collect();
        assert_eq!(words, [0x100, 0x102, 0x101, 0x103]);
        let decoded = NativeMap16Clipboard::decode(&encoded).unwrap();
        assert_eq!(decoded.tiles, clipboard.tiles);
        assert!(decoded.alternate_word_order);
        assert_eq!(decoded.encode().unwrap(), encoded);
    }

    #[test]
    fn native_rectangle_rejects_wrap_overflow_and_malformed_headers() {
        assert!(matches!(
            NativeMap16Clipboard::from_rectangle(0x2f, 2, 1, vec![tile(0), tile(1)]),
            Err(NativeMap16ClipboardError::SourceRectangle { .. })
        ));
        assert!(matches!(
            NativeMap16Clipboard::from_rectangle(
                NativeMap16Clipboard::MAX_TILE_INDEX - 1,
                1,
                2,
                vec![tile(0), tile(1)]
            ),
            Err(NativeMap16ClipboardError::SourceRectangle { .. })
        ));
        assert!(matches!(
            NativeMap16Clipboard::from_rectangle(0, 17, 1, vec![tile(0); 17]),
            Err(NativeMap16ClipboardError::Shape { .. })
        ));

        let valid = NativeMap16Clipboard::from_rectangle(0, 1, 1, vec![tile(0)])
            .unwrap()
            .encode()
            .unwrap();
        assert_eq!(
            NativeMap16Clipboard::decode(&valid[..valid.len() - 1]),
            Err(NativeMap16ClipboardError::Header)
        );
        for (offset, value, expected) in [
            (0x00, 0xa1_u32, NativeMap16ClipboardError::Header),
            (
                0x50,
                2,
                NativeMap16ClipboardError::Shape {
                    selected: 2,
                    width: 1,
                    height: 1,
                    tiles: 1,
                    source_indices: 1,
                },
            ),
            (0x60, 2, NativeMap16ClipboardError::Flags(2)),
        ] {
            let mut malformed = valid.clone();
            malformed[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert_eq!(NativeMap16Clipboard::decode(&malformed), Err(expected));
        }
        let mut reserved = valid;
        reserved[0x20] = 1;
        assert_eq!(
            NativeMap16Clipboard::decode(&reserved),
            Err(NativeMap16ClipboardError::Header)
        );
    }
}
