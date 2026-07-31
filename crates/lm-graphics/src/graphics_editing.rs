use crate::{GraphicsFile4bpp, IndexedTile};
use std::{collections::BTreeSet, fmt};

/// The subsystem that owns one decoded graphics tile in the active editor context.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GraphicsTileOwner {
    #[default]
    Editable,
    Fixed,
    ExAnimation {
        record: u16,
    },
    OriginalAnimation {
        slot: u8,
    },
    LevelExAnimation {
        slot: u8,
    },
    GlobalExAnimation {
        slot: u8,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsOwnership {
    owners: Vec<GraphicsTileOwner>,
}

impl GraphicsOwnership {
    #[must_use]
    pub fn editable(tile_count: usize) -> Self {
        Self {
            owners: vec![GraphicsTileOwner::Editable; tile_count],
        }
    }

    #[must_use]
    pub fn from_owners(owners: Vec<GraphicsTileOwner>) -> Self {
        Self { owners }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.owners.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }

    #[must_use]
    pub fn owner(&self, index: usize) -> Option<GraphicsTileOwner> {
        self.owners.get(index).copied()
    }

    /// Changes the declared owner of an existing tile.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsEditError::TileOutOfRange`] for an invalid index.
    pub fn set_owner(
        &mut self,
        index: usize,
        owner: GraphicsTileOwner,
    ) -> Result<(), GraphicsEditError> {
        let len = self.owners.len();
        let target = self
            .owners
            .get_mut(index)
            .ok_or(GraphicsEditError::TileOutOfRange { index, len })?;
        *target = owner;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsTileChange {
    pub index: usize,
    pub tile: IndexedTile,
}

/// A deterministic match against an existing tile, including required display flips.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EquivalentTile {
    pub index: usize,
    pub x_flip: bool,
    pub y_flip: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsEditError {
    OwnershipShape {
        tiles: usize,
        owners: usize,
    },
    TileOutOfRange {
        index: usize,
        len: usize,
    },
    DuplicateTile(usize),
    ProtectedTile {
        index: usize,
        owner: GraphicsTileOwner,
    },
    PixelOutOfRange {
        index: usize,
        pixel: usize,
        value: u8,
    },
    RangeOverflow,
}

impl fmt::Display for GraphicsEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid graphics edit: {self:?}")
    }
}

impl std::error::Error for GraphicsEditError {}

impl GraphicsFile4bpp {
    /// Atomically replaces unique editable tile targets after validating every 4bpp pixel.
    ///
    /// Fixed tiles and tiles with any animation ownership or attribution are rejected. An empty
    /// batch still validates that the ownership map describes this file exactly.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsEditError`] for ownership-shape, range, duplicate, protection, or pixel
    /// errors. Failure leaves the graphics file unchanged.
    pub fn apply_tile_changes(
        &mut self,
        changes: &[GraphicsTileChange],
        ownership: &GraphicsOwnership,
    ) -> Result<(), GraphicsEditError> {
        validate_ownership_shape(self, ownership)?;
        let mut indexes = BTreeSet::new();
        for change in changes {
            let owner = ownership
                .owner(change.index)
                .ok_or(GraphicsEditError::TileOutOfRange {
                    index: change.index,
                    len: self.tiles.len(),
                })?;
            if !indexes.insert(change.index) {
                return Err(GraphicsEditError::DuplicateTile(change.index));
            }
            if owner != GraphicsTileOwner::Editable {
                return Err(GraphicsEditError::ProtectedTile {
                    index: change.index,
                    owner,
                });
            }
            validate_pixels(change.index, &change.tile)?;
        }
        for change in changes {
            self.tiles[change.index] = change.tile.clone();
        }
        Ok(())
    }

    /// Replaces a contiguous tile range through the same ownership-aware batch path.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsEditError`] for overflow, missing/protected targets, or invalid pixels.
    pub fn replace_tile_range(
        &mut self,
        start: usize,
        tiles: &[IndexedTile],
        ownership: &GraphicsOwnership,
    ) -> Result<(), GraphicsEditError> {
        let end = start
            .checked_add(tiles.len())
            .ok_or(GraphicsEditError::RangeOverflow)?;
        if end > self.tiles.len() {
            return Err(GraphicsEditError::TileOutOfRange {
                index: end.saturating_sub(1),
                len: self.tiles.len(),
            });
        }
        let changes = tiles
            .iter()
            .cloned()
            .enumerate()
            .map(|(offset, tile)| GraphicsTileChange {
                index: start + offset,
                tile,
            })
            .collect::<Vec<_>>();
        self.apply_tile_changes(&changes, ownership)
    }

    /// Finds the lowest-index exact or flip-equivalent existing tile.
    ///
    /// Exact orientation wins over horizontal, vertical, then both flips at each index. The input
    /// is rejected when it is not valid 4bpp indexed data.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsEditError::PixelOutOfRange`] for an invalid input tile.
    pub fn find_equivalent_tile(
        &self,
        tile: &IndexedTile,
    ) -> Result<Option<EquivalentTile>, GraphicsEditError> {
        validate_pixels(usize::MAX, tile)?;
        let variants = [
            (false, false, tile.clone()),
            (true, false, tile.flipped(true, false)),
            (false, true, tile.flipped(false, true)),
            (true, true, tile.flipped(true, true)),
        ];
        for (index, existing) in self.tiles.iter().enumerate() {
            for (x_flip, y_flip, candidate) in &variants {
                if existing == candidate {
                    return Ok(Some(EquivalentTile {
                        index,
                        x_flip: *x_flip,
                        y_flip: *y_flip,
                    }));
                }
            }
        }
        Ok(None)
    }
}

fn validate_ownership_shape(
    graphics: &GraphicsFile4bpp,
    ownership: &GraphicsOwnership,
) -> Result<(), GraphicsEditError> {
    if graphics.tiles.len() != ownership.len() {
        return Err(GraphicsEditError::OwnershipShape {
            tiles: graphics.tiles.len(),
            owners: ownership.len(),
        });
    }
    Ok(())
}

fn validate_pixels(index: usize, tile: &IndexedTile) -> Result<(), GraphicsEditError> {
    if let Some((pixel, value)) = tile
        .pixels()
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value > 0x0f)
    {
        return Err(GraphicsEditError::PixelOutOfRange {
            index,
            pixel,
            value,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GraphicsInterchangeFile;

    fn asymmetric() -> IndexedTile {
        IndexedTile::new(std::array::from_fn(|index| {
            u8::try_from((index * 7 + index / 8) % 16).unwrap()
        }))
    }

    fn graphics() -> GraphicsFile4bpp {
        GraphicsFile4bpp {
            tiles: vec![
                IndexedTile::new([0; 64]),
                asymmetric(),
                IndexedTile::new([2; 64]),
            ],
        }
    }

    #[test]
    fn editable_batch_commits_and_round_trips() {
        let mut graphics = graphics();
        let ownership = GraphicsOwnership::editable(3);
        graphics
            .replace_tile_range(
                1,
                &[IndexedTile::new([4; 64]), IndexedTile::new([5; 64])],
                &ownership,
            )
            .unwrap();
        assert_eq!(graphics.tiles[1].pixels(), &[4; 64]);
        let file = GraphicsInterchangeFile {
            source_slot: 7,
            graphics: graphics.clone(),
        };
        assert_eq!(
            GraphicsInterchangeFile::decode(&file.encode().unwrap())
                .unwrap()
                .graphics,
            graphics
        );
    }

    #[test]
    fn protected_duplicate_invalid_and_range_failures_are_atomic() {
        let mut graphics = graphics();
        let original = graphics.clone();
        let mut ownership = GraphicsOwnership::editable(3);
        ownership
            .set_owner(1, GraphicsTileOwner::ExAnimation { record: 4 })
            .unwrap();
        let protected = GraphicsTileChange {
            index: 1,
            tile: IndexedTile::new([3; 64]),
        };
        assert!(matches!(
            graphics.apply_tile_changes(&[protected], &ownership),
            Err(GraphicsEditError::ProtectedTile { index: 1, .. })
        ));
        let duplicate = GraphicsTileChange {
            index: 0,
            tile: IndexedTile::new([1; 64]),
        };
        assert_eq!(
            graphics.apply_tile_changes(&[duplicate.clone(), duplicate], &ownership),
            Err(GraphicsEditError::DuplicateTile(0))
        );
        assert!(matches!(
            graphics.replace_tile_range(3, &[IndexedTile::new([1; 64])], &ownership),
            Err(GraphicsEditError::TileOutOfRange { .. })
        ));
        assert!(matches!(
            graphics.apply_tile_changes(
                &[GraphicsTileChange {
                    index: 0,
                    tile: IndexedTile::new([16; 64]),
                }],
                &ownership,
            ),
            Err(GraphicsEditError::PixelOutOfRange { .. })
        ));
        assert_eq!(graphics, original);
    }

    #[test]
    fn every_animation_attribution_is_protected() {
        for owner in [
            GraphicsTileOwner::ExAnimation { record: 4 },
            GraphicsTileOwner::OriginalAnimation { slot: 5 },
            GraphicsTileOwner::LevelExAnimation { slot: 6 },
            GraphicsTileOwner::GlobalExAnimation { slot: 7 },
        ] {
            let mut graphics = graphics();
            let original = graphics.clone();
            let mut ownership = GraphicsOwnership::editable(3);
            ownership.set_owner(1, owner).unwrap();
            let change = GraphicsTileChange {
                index: 1,
                tile: IndexedTile::new([3; 64]),
            };
            assert!(matches!(
                graphics.apply_tile_changes(&[change], &ownership),
                Err(GraphicsEditError::ProtectedTile { index: 1, .. })
            ));
            assert_eq!(graphics, original);
        }
    }

    #[test]
    fn flip_equivalence_is_deterministic_and_validated() {
        let graphics = graphics();
        let source = asymmetric();
        assert_eq!(
            graphics.find_equivalent_tile(&source).unwrap(),
            Some(EquivalentTile {
                index: 1,
                x_flip: false,
                y_flip: false,
            })
        );
        assert_eq!(
            graphics
                .find_equivalent_tile(&source.flipped(true, false))
                .unwrap(),
            Some(EquivalentTile {
                index: 1,
                x_flip: true,
                y_flip: false,
            })
        );
        assert!(
            graphics
                .find_equivalent_tile(&IndexedTile::new([9; 64]))
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            graphics.find_equivalent_tile(&IndexedTile::new([16; 64])),
            Err(GraphicsEditError::PixelOutOfRange { .. })
        ));
    }
}
