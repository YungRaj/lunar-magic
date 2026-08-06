use crate::{EquivalentTile, GraphicsFile4bpp, GraphicsOwnership, GraphicsTileOwner, IndexedTile};
use std::fmt;

const MAX_DIMENSION: usize = 4096;
const SNES_TILE_LIMIT: usize = 0x400;

/// Tile-allocation and deduplication behavior for one indexed bitmap import.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexedBitmapImportOptions {
    /// First tile number eligible for new materialization.
    pub allocation_start: usize,
    /// Exclusive tile-number allocation bound.
    pub allocation_end: usize,
    /// Reuse matching tiles that were occupied before this import.
    pub reuse_existing_tiles: bool,
    /// Reuse a matching tile already created earlier in this import.
    pub optimize_new_tiles: bool,
    /// Permit horizontal/vertical flipped matches instead of exact matches only.
    pub allow_flipped_matches: bool,
    /// Route an all-zero source tile directly to this existing tile without allocating or writing.
    pub blank_tile: Option<usize>,
}

impl Default for IndexedBitmapImportOptions {
    fn default() -> Self {
        Self {
            allocation_start: 0,
            allocation_end: SNES_TILE_LIMIT,
            reuse_existing_tiles: true,
            optimize_new_tiles: true,
            allow_flipped_matches: true,
            blank_tile: None,
        }
    }
}

/// One row-major 8×8 bitmap placement after exact/flip-aware tile materialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportedTilePlacement {
    pub tile: u16,
    pub x_flip: bool,
    pub y_flip: bool,
}

/// A staged graphics file and the tile plane that references it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedBitmapImport {
    pub graphics: GraphicsFile4bpp,
    pub occupied: Vec<bool>,
    pub width_in_tiles: usize,
    pub height_in_tiles: usize,
    pub placements: Vec<ImportedTilePlacement>,
}

impl IndexedBitmapImport {
    /// Extracts an indexed bitmap into reusable 8×8 SNES tiles.
    ///
    /// Existing occupied tiles are searched in stable index order. At each index exact,
    /// horizontal, vertical, then dual-flip orientation wins. New tiles use the lowest free,
    /// editable slot below the 10-bit SNES tile-number boundary. All validation and allocation is
    /// staged, so failure leaves the supplied graphics and occupancy map unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`BitmapImportError`] for invalid dimensions/pixel shape, non-4bpp indexes,
    /// occupancy/ownership mismatch, protected or exhausted free space, or arithmetic overflow.
    pub fn materialize(
        width: usize,
        height: usize,
        pixels: &[u8],
        graphics: &GraphicsFile4bpp,
        ownership: &GraphicsOwnership,
        occupied: &[bool],
    ) -> Result<Self, BitmapImportError> {
        let options = IndexedBitmapImportOptions {
            allocation_end: graphics.tiles.len().min(SNES_TILE_LIMIT),
            ..IndexedBitmapImportOptions::default()
        };
        Self::materialize_with_options(
            width, height, pixels, graphics, ownership, occupied, options,
        )
    }

    /// Extracts an indexed bitmap with explicit Lunar Magic-compatible optimization bounds.
    ///
    /// # Errors
    ///
    /// Returns the same validation errors as [`Self::materialize`] and additionally rejects an
    /// inverted or out-of-workspace allocation range.
    pub fn materialize_with_options(
        width: usize,
        height: usize,
        pixels: &[u8],
        graphics: &GraphicsFile4bpp,
        ownership: &GraphicsOwnership,
        occupied: &[bool],
        options: IndexedBitmapImportOptions,
    ) -> Result<Self, BitmapImportError> {
        if width == 0
            || height == 0
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
            || width % IndexedTile::WIDTH != 0
            || height % IndexedTile::HEIGHT != 0
        {
            return Err(BitmapImportError::InvalidDimensions { width, height });
        }
        let expected = width
            .checked_mul(height)
            .ok_or(BitmapImportError::SizeOverflow)?;
        if pixels.len() != expected {
            return Err(BitmapImportError::WrongPixelCount {
                expected,
                actual: pixels.len(),
            });
        }
        if graphics.tiles.len() != ownership.len() || graphics.tiles.len() != occupied.len() {
            return Err(BitmapImportError::ShapeMismatch {
                graphics: graphics.tiles.len(),
                ownership: ownership.len(),
                occupied: occupied.len(),
            });
        }
        let allocation_limit = graphics.tiles.len().min(SNES_TILE_LIMIT);
        if options.allocation_start > options.allocation_end
            || options.allocation_end > allocation_limit
        {
            return Err(BitmapImportError::InvalidAllocationRange {
                start: options.allocation_start,
                end: options.allocation_end,
                limit: allocation_limit,
            });
        }
        if let Some(blank_tile) = options.blank_tile
            && blank_tile >= allocation_limit
        {
            return Err(BitmapImportError::InvalidBlankTile {
                tile: blank_tile,
                limit: allocation_limit,
            });
        }
        if let Some((index, value)) = pixels
            .iter()
            .copied()
            .enumerate()
            .find(|(_, value)| *value > 0x0f)
        {
            return Err(BitmapImportError::PixelOutOfRange { index, value });
        }

        let width_in_tiles = width / IndexedTile::WIDTH;
        let height_in_tiles = height / IndexedTile::HEIGHT;
        let tile_count = width_in_tiles
            .checked_mul(height_in_tiles)
            .ok_or(BitmapImportError::SizeOverflow)?;
        let mut staged_graphics = graphics.clone();
        let mut staged_occupied = occupied.to_vec();
        let mut placements = Vec::with_capacity(tile_count);
        for tile_y in 0..height_in_tiles {
            for tile_x in 0..width_in_tiles {
                let tile = extract_tile(width, pixels, tile_x, tile_y);
                let equivalent = options
                    .blank_tile
                    .filter(|_| tile.pixels().iter().all(|pixel| *pixel == 0))
                    .map(|index| EquivalentTile {
                        index,
                        x_flip: false,
                        y_flip: false,
                    })
                    .or_else(|| {
                        find_reusable_equivalent(
                            &staged_graphics,
                            occupied,
                            &staged_occupied,
                            &tile,
                            options,
                        )
                    });
                let equivalent = if let Some(equivalent) = equivalent {
                    equivalent
                } else {
                    let index = allocate_tile(
                        &mut staged_graphics,
                        ownership,
                        &mut staged_occupied,
                        tile,
                        options.allocation_start,
                        options.allocation_end,
                    )?;
                    EquivalentTile {
                        index,
                        x_flip: false,
                        y_flip: false,
                    }
                };
                placements.push(ImportedTilePlacement {
                    tile: u16::try_from(equivalent.index)
                        .map_err(|_| BitmapImportError::TileNumberOutOfRange(equivalent.index))?,
                    x_flip: equivalent.x_flip,
                    y_flip: equivalent.y_flip,
                });
            }
        }
        Ok(Self {
            graphics: staged_graphics,
            occupied: staged_occupied,
            width_in_tiles,
            height_in_tiles,
            placements,
        })
    }
}

fn extract_tile(width: usize, pixels: &[u8], tile_x: usize, tile_y: usize) -> IndexedTile {
    let mut tile = [0; IndexedTile::PIXEL_COUNT];
    for y in 0..IndexedTile::HEIGHT {
        let source = (tile_y * IndexedTile::HEIGHT + y) * width + tile_x * IndexedTile::WIDTH;
        let target = y * IndexedTile::WIDTH;
        tile[target..target + IndexedTile::WIDTH]
            .copy_from_slice(&pixels[source..source + IndexedTile::WIDTH]);
    }
    IndexedTile::new(tile)
}

fn find_reusable_equivalent(
    graphics: &GraphicsFile4bpp,
    initially_occupied: &[bool],
    staged_occupied: &[bool],
    tile: &IndexedTile,
    options: IndexedBitmapImportOptions,
) -> Option<EquivalentTile> {
    let variants = [
        (false, false, tile.clone()),
        (true, false, tile.flipped(true, false)),
        (false, true, tile.flipped(false, true)),
        (true, true, tile.flipped(true, true)),
    ];
    graphics
        .tiles
        .iter()
        .zip(initially_occupied.iter().zip(staged_occupied))
        .take(SNES_TILE_LIMIT)
        .enumerate()
        .find_map(
            |(index, (existing, (initially_occupied, staged_occupied)))| {
                let reusable = (*initially_occupied && options.reuse_existing_tiles)
                    || (!*initially_occupied && *staged_occupied && options.optimize_new_tiles);
                reusable.then(|| {
                    variants.iter().find_map(|(x_flip, y_flip, candidate)| {
                        let orientation_allowed =
                            (!*x_flip && !*y_flip) || options.allow_flipped_matches;
                        (orientation_allowed && existing == candidate).then_some(EquivalentTile {
                            index,
                            x_flip: *x_flip,
                            y_flip: *y_flip,
                        })
                    })
                })?
            },
        )
}

fn allocate_tile(
    graphics: &mut GraphicsFile4bpp,
    ownership: &GraphicsOwnership,
    occupied: &mut [bool],
    tile: IndexedTile,
    start: usize,
    end: usize,
) -> Result<usize, BitmapImportError> {
    let mut saw_free_protected = false;
    for (index, slot_occupied) in occupied.iter_mut().enumerate().take(end).skip(start) {
        if *slot_occupied {
            continue;
        }
        if ownership.owner(index) != Some(GraphicsTileOwner::Editable) {
            saw_free_protected = true;
            continue;
        }
        graphics.tiles[index] = tile;
        *slot_occupied = true;
        return Ok(index);
    }
    if saw_free_protected {
        Err(BitmapImportError::OnlyProtectedSlotsRemain)
    } else {
        Err(BitmapImportError::NoFreeTile)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BitmapImportError {
    InvalidDimensions {
        width: usize,
        height: usize,
    },
    WrongPixelCount {
        expected: usize,
        actual: usize,
    },
    ShapeMismatch {
        graphics: usize,
        ownership: usize,
        occupied: usize,
    },
    PixelOutOfRange {
        index: usize,
        value: u8,
    },
    InvalidAllocationRange {
        start: usize,
        end: usize,
        limit: usize,
    },
    InvalidBlankTile {
        tile: usize,
        limit: usize,
    },
    TileNumberOutOfRange(usize),
    OnlyProtectedSlotsRemain,
    NoFreeTile,
    SizeOverflow,
}

impl fmt::Display for BitmapImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "indexed bitmap tile import failed: {self:?}")
    }
}

impl std::error::Error for BitmapImportError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn asymmetric() -> IndexedTile {
        IndexedTile::new(std::array::from_fn(|index| {
            u8::try_from((index * 5 + index / 8) % 16).unwrap()
        }))
    }

    fn side_by_side(left: &IndexedTile, right: &IndexedTile) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(128);
        for row in 0..8 {
            pixels.extend_from_slice(&left.pixels()[row * 8..row * 8 + 8]);
            pixels.extend_from_slice(&right.pixels()[row * 8..row * 8 + 8]);
        }
        pixels
    }

    #[test]
    fn occupied_flip_matches_are_reused_before_lowest_free_allocation() {
        let existing = asymmetric();
        let novel = IndexedTile::new([7; 64]);
        let graphics = GraphicsFile4bpp {
            tiles: vec![
                IndexedTile::new([0; 64]),
                existing.clone(),
                IndexedTile::new([0; 64]),
            ],
        };
        let pixels = side_by_side(&existing.flipped(true, false), &novel);
        let result = IndexedBitmapImport::materialize(
            16,
            8,
            &pixels,
            &graphics,
            &GraphicsOwnership::editable(3),
            &[false, true, false],
        )
        .unwrap();
        assert_eq!(
            result.placements,
            [
                ImportedTilePlacement {
                    tile: 1,
                    x_flip: true,
                    y_flip: false,
                },
                ImportedTilePlacement {
                    tile: 0,
                    x_flip: false,
                    y_flip: false,
                },
            ]
        );
        assert_eq!(result.graphics.tiles[0], novel);
        assert_eq!(result.occupied, [true, true, false]);
    }

    #[test]
    fn new_duplicates_reuse_the_first_allocated_tile() {
        let pixels = vec![3; 16 * 8];
        let result = IndexedBitmapImport::materialize(
            16,
            8,
            &pixels,
            &GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([0; 64]); 2],
            },
            &GraphicsOwnership::editable(2),
            &[false, false],
        )
        .unwrap();
        assert_eq!(result.placements[0].tile, 0);
        assert_eq!(result.placements[1].tile, 0);
        assert_eq!(result.occupied, [true, false]);
    }

    #[test]
    fn configured_blank_tile_bypasses_allocation_and_ownership() {
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([7; 64]), IndexedTile::new([0; 64])],
        };
        let ownership = GraphicsOwnership::from_owners(vec![
            GraphicsTileOwner::Fixed,
            GraphicsTileOwner::Fixed,
        ]);
        let options = IndexedBitmapImportOptions {
            allocation_start: 0,
            allocation_end: 2,
            reuse_existing_tiles: false,
            optimize_new_tiles: false,
            allow_flipped_matches: false,
            blank_tile: Some(1),
        };

        let result = IndexedBitmapImport::materialize_with_options(
            8,
            8,
            &[0; 64],
            &graphics,
            &ownership,
            &[true, false],
            options,
        )
        .unwrap();

        assert_eq!(result.placements[0].tile, 1);
        assert_eq!(result.graphics, graphics);
        assert_eq!(result.occupied, [true, false]);
    }

    #[test]
    fn configured_blank_tile_is_bounded_by_the_graphics_workspace() {
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; 64])],
        };
        let options = IndexedBitmapImportOptions {
            allocation_end: 1,
            blank_tile: Some(1),
            ..IndexedBitmapImportOptions::default()
        };

        let error = IndexedBitmapImport::materialize_with_options(
            8,
            8,
            &[0; 64],
            &graphics,
            &GraphicsOwnership::editable(1),
            &[false],
            options,
        )
        .unwrap_err();

        assert_eq!(
            error,
            BitmapImportError::InvalidBlankTile { tile: 1, limit: 1 }
        );
    }

    #[test]
    fn late_exhaustion_and_malformed_inputs_are_atomic() {
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; 64])],
        };
        let pixels = side_by_side(&IndexedTile::new([1; 64]), &IndexedTile::new([2; 64]));
        assert_eq!(
            IndexedBitmapImport::materialize(
                16,
                8,
                &pixels,
                &graphics,
                &GraphicsOwnership::editable(1),
                &[false]
            ),
            Err(BitmapImportError::NoFreeTile)
        );
        assert_eq!(graphics.tiles[0], IndexedTile::new([0; 64]));
        assert!(matches!(
            IndexedBitmapImport::materialize(
                7,
                8,
                &[0; 56],
                &graphics,
                &GraphicsOwnership::editable(1),
                &[false]
            ),
            Err(BitmapImportError::InvalidDimensions { .. })
        ));
        let mut invalid = [0; 64];
        invalid[63] = 16;
        assert!(matches!(
            IndexedBitmapImport::materialize(
                8,
                8,
                &invalid,
                &graphics,
                &GraphicsOwnership::editable(1),
                &[false]
            ),
            Err(BitmapImportError::PixelOutOfRange { index: 63, .. })
        ));
    }

    #[test]
    fn allocation_bounds_and_independent_optimization_switches_are_exact() {
        let repeated = IndexedTile::new([7; 64]);
        let pixels = side_by_side(&repeated, &repeated);
        let graphics = GraphicsFile4bpp {
            tiles: vec![
                repeated.clone(),
                IndexedTile::new([0; 64]),
                IndexedTile::new([0; 64]),
                IndexedTile::new([0; 64]),
            ],
        };
        let ownership = GraphicsOwnership::editable(4);
        let occupied = [true, false, false, false];

        let no_reuse = IndexedBitmapImport::materialize_with_options(
            16,
            8,
            &pixels,
            &graphics,
            &ownership,
            &occupied,
            IndexedBitmapImportOptions {
                allocation_start: 2,
                allocation_end: 4,
                reuse_existing_tiles: false,
                optimize_new_tiles: false,
                allow_flipped_matches: true,
                blank_tile: None,
            },
        )
        .unwrap();
        assert_eq!(
            no_reuse
                .placements
                .iter()
                .map(|placement| placement.tile)
                .collect::<Vec<_>>(),
            [2, 3]
        );

        let new_only = IndexedBitmapImport::materialize_with_options(
            16,
            8,
            &pixels,
            &graphics,
            &ownership,
            &occupied,
            IndexedBitmapImportOptions {
                allocation_start: 2,
                allocation_end: 4,
                reuse_existing_tiles: false,
                optimize_new_tiles: true,
                allow_flipped_matches: true,
                blank_tile: None,
            },
        )
        .unwrap();
        assert_eq!(
            new_only
                .placements
                .iter()
                .map(|placement| placement.tile)
                .collect::<Vec<_>>(),
            [2, 2]
        );

        let existing = IndexedBitmapImport::materialize_with_options(
            16,
            8,
            &pixels,
            &graphics,
            &ownership,
            &occupied,
            IndexedBitmapImportOptions {
                allocation_start: 2,
                allocation_end: 4,
                reuse_existing_tiles: true,
                optimize_new_tiles: false,
                allow_flipped_matches: false,
                blank_tile: None,
            },
        )
        .unwrap();
        assert!(
            existing
                .placements
                .iter()
                .all(|placement| placement.tile == 0)
        );
    }

    #[test]
    fn invalid_allocation_ranges_are_rejected_before_materialization() {
        let graphics = GraphicsFile4bpp {
            tiles: vec![IndexedTile::new([0; 64]); 3],
        };
        let error = IndexedBitmapImport::materialize_with_options(
            8,
            8,
            &[1; 64],
            &graphics,
            &GraphicsOwnership::editable(3),
            &[false; 3],
            IndexedBitmapImportOptions {
                allocation_start: 2,
                allocation_end: 4,
                ..IndexedBitmapImportOptions::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            BitmapImportError::InvalidAllocationRange {
                start: 2,
                end: 4,
                limit: 3
            }
        );
    }
}
