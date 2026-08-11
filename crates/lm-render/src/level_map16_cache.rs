use std::fmt;

pub const LEVEL_MAP16_CACHE_CELLS: usize = 0x3800;
pub const LEVEL_MAP16_CACHE_SENTINEL: usize = LEVEL_MAP16_CACHE_CELLS;

/// Runtime layout inputs recovered from `CalculateLevelMap16CellIndex`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLevelMap16Layout {
    pub width: usize,
    pub height: usize,
    pub page_stride: usize,
    pub base_cell: usize,
    pub vertical: bool,
}

#[derive(Clone, Debug)]
pub struct NativeLevelMap16Cache {
    cells: Vec<u16>,
    written: Vec<bool>,
    writes: Vec<NativeLevelMap16Write>,
    bounds_flags: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeLevelMap16Write {
    pub index: usize,
    pub tile: u16,
}

impl PartialEq for NativeLevelMap16Cache {
    fn eq(&self, other: &Self) -> bool {
        self.cells == other.cells
    }
}

impl Eq for NativeLevelMap16Cache {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeLevelMap16CacheError {
    InvalidLength(usize),
    CoordinateOverflow,
    CellOutOfRange(usize),
}

impl fmt::Display for NativeLevelMap16CacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native level Map16 cache: {self:?}")
    }
}

impl std::error::Error for NativeLevelMap16CacheError {}

impl NativeLevelMap16Cache {
    #[must_use]
    pub fn filled(tile: u16) -> Self {
        Self {
            cells: vec![tile; LEVEL_MAP16_CACHE_CELLS],
            written: vec![false; LEVEL_MAP16_CACHE_CELLS],
            writes: Vec::new(),
            bounds_flags: 0,
        }
    }

    /// Decodes the debugger-visible little-endian cache.
    ///
    /// # Errors
    ///
    /// Requires exactly 0x3800 words.
    pub fn decode(bytes: &[u8]) -> Result<Self, NativeLevelMap16CacheError> {
        if bytes.len() != LEVEL_MAP16_CACHE_CELLS * 2 {
            return Err(NativeLevelMap16CacheError::InvalidLength(bytes.len()));
        }
        Ok(Self {
            cells: bytes
                .chunks_exact(2)
                .map(|word| u16::from_le_bytes([word[0], word[1]]))
                .collect(),
            written: vec![false; LEVEL_MAP16_CACHE_CELLS],
            writes: Vec::new(),
            bounds_flags: 0,
        })
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.cells
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    #[must_use]
    pub fn cells(&self) -> &[u16] {
        &self.cells
    }

    /// Matches Lunar Magic's renderer-side placement diagnostic: bit 0 means a tile was painted
    /// before the first top/left edge, and bit 1 means one was painted beyond the last
    /// bottom/right edge. Clipped cells remain absent from the cache just as they do in Lunar
    /// Magic; this state only records why they were clipped.
    #[must_use]
    pub const fn bounds_flags(&self) -> u8 {
        self.bounds_flags
    }

    pub(crate) fn mark_before_first_boundary(&mut self) {
        self.bounds_flags |= 1;
    }

    pub(crate) fn mark_after_last_boundary(&mut self) {
        self.bounds_flags |= 2;
    }

    /// Returns every explicit cache write in execution order.
    #[must_use]
    pub fn writes(&self) -> &[NativeLevelMap16Write] {
        &self.writes
    }

    /// Overlays only cells explicitly written while constructing `source`.
    ///
    /// Lunar Magic shares one physical cache between Layer 1 and object-based Layer 2. Rendering
    /// each stream independently and merging only written cells reproduces that partition without
    /// allowing either renderer's blank initialization to erase the other layer.
    pub fn overlay_written_cells(&mut self, source: &Self) {
        self.bounds_flags |= source.bounds_flags;
        for index in 0..LEVEL_MAP16_CACHE_CELLS {
            if source.written[index] {
                self.cells[index] = source.cells[index];
                self.written[index] = true;
            }
        }
        self.writes.extend_from_slice(&source.writes);
    }

    /// Fills rows in a consecutive set of horizontal `$1B0`-word screen pages.
    ///
    /// Lunar Magic uses this to clear the eleven non-layer rows in the second half of the shared
    /// cache when a horizontal level is split into sixteen Layer 1 and sixteen Layer 2 screens.
    pub fn fill_horizontal_screen_rows(
        &mut self,
        screens: std::ops::Range<usize>,
        rows: std::ops::Range<usize>,
        tile: u16,
    ) {
        for screen in screens {
            for row in rows.clone() {
                let start = screen
                    .saturating_mul(0x1b0)
                    .saturating_add(row.saturating_mul(16));
                let end = start.saturating_add(16).min(LEVEL_MAP16_CACHE_CELLS);
                if let Some(cells) = self.cells.get_mut(start..end) {
                    cells.fill(tile);
                    self.written[start..end].fill(true);
                    self.writes
                        .extend((start..end).map(|index| NativeLevelMap16Write { index, tile }));
                }
            }
        }
    }

    /// Reports whether a renderer explicitly wrote the indexed cell after cache construction.
    #[must_use]
    pub fn was_written(&self, index: usize) -> bool {
        self.written.get(index).copied().unwrap_or(false)
    }

    pub(crate) fn raw_get(&self, index: usize) -> Result<u16, NativeLevelMap16CacheError> {
        self.cells
            .get(index)
            .copied()
            .ok_or(NativeLevelMap16CacheError::CellOutOfRange(index))
    }

    pub(crate) fn raw_set(
        &mut self,
        index: usize,
        tile: u16,
    ) -> Result<(), NativeLevelMap16CacheError> {
        let cell = self
            .cells
            .get_mut(index)
            .ok_or(NativeLevelMap16CacheError::CellOutOfRange(index))?;
        *cell = tile;
        self.written[index] = true;
        self.writes.push(NativeLevelMap16Write { index, tile });
        Ok(())
    }

    /// Reads one mapped cell.
    ///
    /// # Errors
    ///
    /// Rejects coordinates that map to Lunar Magic's 0x3800 sentinel.
    pub fn get(
        &self,
        layout: NativeLevelMap16Layout,
        x: usize,
        y: usize,
    ) -> Result<u16, NativeLevelMap16CacheError> {
        let index = Self::cell_index(layout, x, y);
        self.cells
            .get(index)
            .copied()
            .ok_or(NativeLevelMap16CacheError::CellOutOfRange(index))
    }

    /// Converts tile coordinates with Lunar Magic's horizontal/vertical cache formulas.
    #[must_use]
    pub fn cell_index(layout: NativeLevelMap16Layout, x: usize, y: usize) -> usize {
        if x >= layout.width || y >= layout.height || (!layout.vertical && layout.page_stride < 16)
        {
            return LEVEL_MAP16_CACHE_SENTINEL;
        }
        let relative = if layout.vertical {
            (y >> 4)
                .checked_mul(0x200)
                .and_then(|value| value.checked_add((x >> 4).saturating_mul(0x100)))
                .and_then(|value| value.checked_add((y & 0x0f).saturating_mul(16)))
                .and_then(|value| value.checked_add(x & 0x0f))
        } else {
            (x >> 4)
                .checked_mul(layout.page_stride)
                .and_then(|value| value.checked_add(y.saturating_mul(16)))
                .and_then(|value| value.checked_add(x & 0x0f))
        };
        relative
            .and_then(|value| value.checked_add(layout.base_cell))
            .filter(|index| *index < LEVEL_MAP16_CACHE_CELLS)
            .unwrap_or(LEVEL_MAP16_CACHE_SENTINEL)
    }

    /// Writes one mapped cell.
    ///
    /// # Errors
    ///
    /// Rejects coordinates that map to Lunar Magic's 0x3800 sentinel.
    pub fn set(
        &mut self,
        layout: NativeLevelMap16Layout,
        x: usize,
        y: usize,
        tile: u16,
    ) -> Result<(), NativeLevelMap16CacheError> {
        let index = Self::cell_index(layout, x, y);
        let cell = self
            .cells
            .get_mut(index)
            .ok_or(NativeLevelMap16CacheError::CellOutOfRange(index))?;
        *cell = tile;
        self.written[index] = true;
        self.writes.push(NativeLevelMap16Write { index, tile });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn horizontal() -> NativeLevelMap16Layout {
        NativeLevelMap16Layout {
            width: 32,
            height: 32,
            page_stride: 0x1b0,
            base_cell: 0,
            vertical: false,
        }
    }

    #[test]
    fn recovered_horizontal_and_vertical_index_formulas_are_exact() {
        assert_eq!(NativeLevelMap16Cache::cell_index(horizontal(), 2, 3), 50);
        assert_eq!(NativeLevelMap16Cache::cell_index(horizontal(), 2, 19), 306);
        let vertical = NativeLevelMap16Layout {
            vertical: true,
            ..horizontal()
        };
        assert_eq!(NativeLevelMap16Cache::cell_index(vertical, 2, 3), 50);
        assert_eq!(NativeLevelMap16Cache::cell_index(vertical, 2, 19), 562);
        assert_eq!(
            NativeLevelMap16Cache::cell_index(horizontal(), 32, 0),
            LEVEL_MAP16_CACHE_SENTINEL
        );
    }

    #[test]
    fn live_cache_bytes_round_trip_and_bounds_are_typed() {
        let mut cache = NativeLevelMap16Cache::filled(0x25);
        let written = NativeLevelMap16Cache::cell_index(horizontal(), 2, 3);
        assert!(!cache.was_written(written));
        cache.set(horizontal(), 2, 3, 0x142).unwrap();
        assert!(cache.was_written(written));
        let bytes = cache.encode();
        let decoded = NativeLevelMap16Cache::decode(&bytes).unwrap();
        assert_eq!(decoded, cache);
        assert!(!decoded.was_written(written));
        assert_eq!(
            cache.set(horizontal(), 99, 0, 1),
            Err(NativeLevelMap16CacheError::CellOutOfRange(
                LEVEL_MAP16_CACHE_SENTINEL
            ))
        );
    }

    #[test]
    fn explicit_write_history_preserves_duplicate_painter_order() {
        let mut cache = NativeLevelMap16Cache::filled(0x25);
        let index = NativeLevelMap16Cache::cell_index(horizontal(), 2, 3);
        cache.set(horizontal(), 2, 3, 0x123).unwrap();
        cache.set(horizontal(), 2, 3, 0x456).unwrap();

        assert_eq!(
            cache.writes(),
            [
                NativeLevelMap16Write { index, tile: 0x123 },
                NativeLevelMap16Write { index, tile: 0x456 },
            ]
        );
    }

    #[test]
    fn written_overlay_preserves_other_cells() {
        let mut destination = NativeLevelMap16Cache::filled(0x25);
        destination.set(horizontal(), 1, 1, 0x123).unwrap();
        let mut source = NativeLevelMap16Cache::filled(0);
        source.set(horizontal(), 2, 2, 0x456).unwrap();

        destination.overlay_written_cells(&source);

        assert_eq!(destination.get(horizontal(), 1, 1).unwrap(), 0x123);
        assert_eq!(destination.get(horizontal(), 2, 2).unwrap(), 0x456);
        assert_eq!(destination.get(horizontal(), 3, 3).unwrap(), 0x25);
    }

    #[test]
    fn fills_only_selected_horizontal_screen_rows() {
        let mut cache = NativeLevelMap16Cache::filled(0x25);
        cache.fill_horizontal_screen_rows(16..32, 16..27, 0);

        assert_eq!(cache.cells()[0x1b00 + 0xff], 0x25);
        assert_eq!(cache.cells()[0x1b00 + 0x100], 0);
        assert_eq!(cache.cells()[0x1b00 + 0x1af], 0);
        assert_eq!(cache.cells()[0x3600], 0x25);
        assert!(cache.was_written(0x1c00));
    }
}
