use crate::IndexedTile;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsColorMapFilters {
    mappings: [[u8; Self::COLORS]; Self::FILTERS],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsColorMapError {
    FilterOutOfRange(usize),
    SourceOutOfRange(u8),
    DestinationOutOfRange(u8),
}

impl std::fmt::Display for GraphicsColorMapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid graphics color-map edit: {self:?}")
    }
}

impl std::error::Error for GraphicsColorMapError {}

impl Default for GraphicsColorMapFilters {
    fn default() -> Self {
        Self {
            mappings: [GraphicsColorMapFilters::IDENTITY; GraphicsColorMapFilters::FILTERS],
        }
    }
}

impl GraphicsColorMapFilters {
    pub const FILTERS: usize = 16;
    pub const COLORS: usize = 16;
    const IDENTITY: [u8; Self::COLORS] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

    #[must_use]
    pub fn destination(&self, filter: usize, source: u8) -> Option<u8> {
        self.mappings
            .get(filter)
            .and_then(|mapping| mapping.get(usize::from(source)))
            .copied()
    }

    /// Changes one source-to-destination entry in one filter.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsColorMapError`] when any index is outside its recovered 0–15 range.
    pub fn set_destination(
        &mut self,
        filter: usize,
        source: u8,
        destination: u8,
    ) -> Result<(), GraphicsColorMapError> {
        let mapping = self
            .mappings
            .get_mut(filter)
            .ok_or(GraphicsColorMapError::FilterOutOfRange(filter))?;
        let entry = mapping
            .get_mut(usize::from(source))
            .ok_or(GraphicsColorMapError::SourceOutOfRange(source))?;
        if usize::from(destination) >= Self::COLORS {
            return Err(GraphicsColorMapError::DestinationOutOfRange(destination));
        }
        *entry = destination;
        Ok(())
    }

    /// Restores one filter to the identity mapping.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsColorMapError::FilterOutOfRange`] for a filter above 15.
    pub fn reset(&mut self, filter: usize) -> Result<(), GraphicsColorMapError> {
        let mapping = self
            .mappings
            .get_mut(filter)
            .ok_or(GraphicsColorMapError::FilterOutOfRange(filter))?;
        *mapping = Self::IDENTITY;
        Ok(())
    }

    #[must_use]
    pub fn apply(&self, filter: usize, tile: &IndexedTile) -> Option<IndexedTile> {
        let mapping = self.mappings.get(filter)?;
        Some(IndexedTile::new(std::array::from_fn(|index| {
            mapping[usize::from(tile.pixels()[index] & 0x0f)]
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixteen_identity_filters_remap_every_tile_pixel_independently() {
        let mut filters = GraphicsColorMapFilters::default();
        let tile = IndexedTile::new(std::array::from_fn(|index| index.to_le_bytes()[0] & 0x0f));
        for filter in 0..GraphicsColorMapFilters::FILTERS {
            assert_eq!(filters.apply(filter, &tile), Some(tile.clone()));
        }

        filters.set_destination(3, 1, 14).unwrap();
        let mapped = filters.apply(3, &tile).unwrap();
        for (before, after) in tile.pixels().iter().zip(mapped.pixels()) {
            assert_eq!(*after, if *before == 1 { 14 } else { *before });
        }
        assert_eq!(filters.apply(2, &tile), Some(tile));
    }

    #[test]
    fn reset_and_all_three_indexes_are_strictly_bounded() {
        let mut filters = GraphicsColorMapFilters::default();
        filters.set_destination(15, 15, 0).unwrap();
        filters.reset(15).unwrap();
        assert_eq!(filters.destination(15, 15), Some(15));
        assert_eq!(filters.destination(16, 0), None);
        assert_eq!(filters.destination(0, 16), None);
        assert_eq!(
            filters.set_destination(16, 0, 0),
            Err(GraphicsColorMapError::FilterOutOfRange(16))
        );
        assert_eq!(
            filters.set_destination(0, 16, 0),
            Err(GraphicsColorMapError::SourceOutOfRange(16))
        );
        assert_eq!(
            filters.set_destination(0, 0, 16),
            Err(GraphicsColorMapError::DestinationOutOfRange(16))
        );
        assert_eq!(
            filters.reset(16),
            Err(GraphicsColorMapError::FilterOutOfRange(16))
        );
        assert_eq!(filters.apply(16, &IndexedTile::new([0; 64])), None);
    }
}
