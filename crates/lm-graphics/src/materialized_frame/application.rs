use super::{GraphicsFile4bpp, MaterializedAnimationFrame, MaterializedFrameError, Palette};

impl MaterializedAnimationFrame {
    /// Applies all overrides to cloned assets after validating every target.
    ///
    /// # Errors
    ///
    /// Returns [`MaterializedFrameError`] without changing either input when a target or encoded
    /// value is invalid.
    pub fn apply(
        &self,
        graphics: &GraphicsFile4bpp,
        palette: &Palette,
    ) -> Result<(GraphicsFile4bpp, Palette), MaterializedFrameError> {
        self.validate_values()?;
        for entry in &self.tile_overrides {
            if usize::try_from(entry.tile_index).map_or(true, |index| index >= graphics.tiles.len())
            {
                return Err(MaterializedFrameError::TileTargetOutOfRange {
                    index: entry.tile_index,
                    len: graphics.tiles.len(),
                });
            }
        }
        for entry in &self.palette_overrides {
            if usize::try_from(entry.color_index)
                .map_or(true, |index| index >= palette.colors.len())
            {
                return Err(MaterializedFrameError::ColorTargetOutOfRange {
                    index: entry.color_index,
                    len: palette.colors.len(),
                });
            }
        }

        let mut graphics = graphics.clone();
        let mut palette = palette.clone();
        for entry in &self.tile_overrides {
            graphics.tiles[entry.tile_index as usize] = entry.tile.clone();
        }
        for entry in &self.palette_overrides {
            palette.colors[entry.color_index as usize] = entry.color;
        }
        Ok((graphics, palette))
    }
}
