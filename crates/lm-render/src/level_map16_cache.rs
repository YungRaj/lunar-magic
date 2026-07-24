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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeLevelMap16Cache {
    cells: Vec<u16>,
}

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

    /// Converts tile coordinates with Lunar Magic's horizontal/vertical cache formulas.
    #[must_use]
    pub fn cell_index(layout: NativeLevelMap16Layout, x: usize, y: usize) -> usize {
        if x >= layout.width || y >= layout.height || layout.page_stride < 16 {
            return LEVEL_MAP16_CACHE_SENTINEL;
        }
        let relative = if layout.vertical {
            (x & !0x0f)
                .checked_add((y >> 4).saturating_mul(15))
                .and_then(|value| value.checked_mul(16))
                .and_then(|value| value.checked_add(x.saturating_mul(16)))
                .and_then(|value| value.checked_add(y))
        } else {
            x.checked_mul(16)
                .and_then(|value| value.checked_add(y))
                .and_then(|value| {
                    value.checked_add((layout.page_stride - 16).saturating_mul(y >> 4))
                })
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
        assert_eq!(NativeLevelMap16Cache::cell_index(horizontal(), 2, 3), 35);
        assert_eq!(
            NativeLevelMap16Cache::cell_index(horizontal(), 2, 19),
            51 + 0x1a0
        );
        let vertical = NativeLevelMap16Layout {
            vertical: true,
            ..horizontal()
        };
        assert_eq!(NativeLevelMap16Cache::cell_index(vertical, 2, 3), 35);
        assert_eq!(NativeLevelMap16Cache::cell_index(vertical, 2, 19), 291);
        assert_eq!(
            NativeLevelMap16Cache::cell_index(horizontal(), 32, 0),
            LEVEL_MAP16_CACHE_SENTINEL
        );
    }

    #[test]
    fn live_cache_bytes_round_trip_and_bounds_are_typed() {
        let mut cache = NativeLevelMap16Cache::filled(0x25);
        cache.set(horizontal(), 2, 3, 0x142).unwrap();
        let bytes = cache.encode();
        assert_eq!(NativeLevelMap16Cache::decode(&bytes).unwrap(), cache);
        assert_eq!(
            cache.set(horizontal(), 99, 0, 1),
            Err(NativeLevelMap16CacheError::CellOutOfRange(
                LEVEL_MAP16_CACHE_SENTINEL
            ))
        );
    }
}
