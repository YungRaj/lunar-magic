use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_graphics::IndexedTile;

impl ClipboardPayload {
    #[must_use]
    pub fn from_graphics_tiles(tiles: &[IndexedTile]) -> Self {
        Self {
            kind: ClipboardKind::GraphicsTiles,
            records: tiles.iter().map(|tile| tile.pixels().to_vec()).collect(),
        }
    }

    /// Decodes validated 4bpp indexed tiles.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for wrong sizes or color indexes above 15.
    pub fn to_graphics_tiles(&self) -> Result<Vec<IndexedTile>, ClipboardError> {
        self.require_kind(ClipboardKind::GraphicsTiles)?;
        self.records
            .iter()
            .enumerate()
            .map(|(record_index, record)| {
                let pixels: [u8; IndexedTile::PIXEL_COUNT] =
                    record
                        .as_slice()
                        .try_into()
                        .map_err(|_| ClipboardError::InvalidRecord {
                            index: record_index,
                            length: record.len(),
                        })?;
                if let Some((pixel, value)) = pixels
                    .iter()
                    .copied()
                    .enumerate()
                    .find(|(_, value)| *value > 15)
                {
                    return Err(ClipboardError::InvalidPixel {
                        record: record_index,
                        pixel,
                        value,
                    });
                }
                Ok(IndexedTile::new(pixels))
            })
            .collect()
    }
}
