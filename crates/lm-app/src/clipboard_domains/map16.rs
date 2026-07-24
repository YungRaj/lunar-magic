use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_level::Map16Tile;

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
