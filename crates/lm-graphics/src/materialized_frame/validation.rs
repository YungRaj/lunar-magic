use super::{MaterializedAnimationFrame, MaterializedFrameError};
use std::collections::BTreeSet;

impl MaterializedAnimationFrame {
    pub(super) fn validate_values(&self) -> Result<(), MaterializedFrameError> {
        validate_counts(self.tile_overrides.len(), self.palette_overrides.len())?;
        let mut tiles = BTreeSet::new();
        for entry in &self.tile_overrides {
            if !tiles.insert(entry.tile_index) {
                return Err(MaterializedFrameError::DuplicateTile(entry.tile_index));
            }
            if let Some(pixel) = entry
                .tile
                .pixels()
                .iter()
                .copied()
                .find(|pixel| *pixel > 0x0f)
            {
                return Err(MaterializedFrameError::PixelOutOfRange {
                    tile_index: entry.tile_index,
                    pixel,
                });
            }
        }
        let mut colors = BTreeSet::new();
        for entry in &self.palette_overrides {
            if !colors.insert(entry.color_index) {
                return Err(MaterializedFrameError::DuplicateColor(entry.color_index));
            }
            if entry.color.0 > 0x7fff {
                return Err(MaterializedFrameError::ColorValueOutOfRange {
                    color_index: entry.color_index,
                    value: entry.color.0,
                });
            }
        }
        Ok(())
    }
}

pub(super) fn validate_counts(
    tile_count: usize,
    palette_count: usize,
) -> Result<(), MaterializedFrameError> {
    if tile_count > MaterializedAnimationFrame::MAX_TILE_OVERRIDES {
        return Err(MaterializedFrameError::TooManyTileOverrides(tile_count));
    }
    if palette_count > MaterializedAnimationFrame::MAX_PALETTE_OVERRIDES {
        return Err(MaterializedFrameError::TooManyPaletteOverrides(
            palette_count,
        ));
    }
    Ok(())
}

pub(super) fn encoded_len(
    tile_count: usize,
    palette_count: usize,
) -> Result<usize, MaterializedFrameError> {
    validate_counts(tile_count, palette_count)?;
    tile_count
        .checked_mul(MaterializedAnimationFrame::TILE_ENTRY_LEN)
        .and_then(|tiles| {
            palette_count
                .checked_mul(MaterializedAnimationFrame::PALETTE_ENTRY_LEN)
                .and_then(|colors| tiles.checked_add(colors))
        })
        .and_then(|payload| payload.checked_add(MaterializedAnimationFrame::HEADER_LEN))
        .ok_or(MaterializedFrameError::LengthOverflow)
}
