//! Lunar Magic-compatible native FG/BG graphics workspace for bitmap-to-Map16 imports.

use lm_graphics::{GraphicsFile4bpp, GraphicsOwnership, GraphicsTileOwner, IndexedTile};
use std::fmt;

/// Lunar Magic materializes six consecutive FG/BG slots for bitmap conversion.
pub const NATIVE_MAP16_BITMAP_SLOT_COUNT: usize = 6;
/// Each decoded 4bpp FG/BG slot contains `$1000` bytes, or `$80` 8×8 tiles.
pub const NATIVE_MAP16_BITMAP_TILES_PER_SLOT: usize = 0x80;
/// Total tile-number space scanned by Lunar Magic's bitmap importer.
pub const NATIVE_MAP16_BITMAP_TILE_COUNT: usize =
    NATIVE_MAP16_BITMAP_SLOT_COUNT * NATIVE_MAP16_BITMAP_TILES_PER_SLOT;
/// The original importer's default first allocation candidate.
pub const NATIVE_MAP16_BITMAP_ALLOCATION_START: usize = 0x200;
/// Exclusive end of the six-slot native workspace.
pub const NATIVE_MAP16_BITMAP_ALLOCATION_END: usize = 0x300;
/// Default tile used when an imported 8×8 region is blank.
pub const NATIVE_MAP16_BITMAP_BLANK_TILE: usize = 0x0f8;

/// One native FG/BG graphics workspace assembled in VRAM tile-number order.
///
/// `assignments` retains the ROM GFX/ExGFX file behind each slot. A `None` assignment models
/// Lunar Magic's `$7f` sentinel: it displays as a blank `$80`-tile slot, but cannot be persisted
/// directly until the caller assigns a concrete graphics file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMap16BitmapGraphicsWorkspace {
    pub assignments: [Option<usize>; NATIVE_MAP16_BITMAP_SLOT_COUNT],
    pub graphics: GraphicsFile4bpp,
    pub ownership: GraphicsOwnership,
    pub occupied: Vec<bool>,
}

impl NativeMap16BitmapGraphicsWorkspace {
    /// Assembles six exact `$80`-tile slots into Lunar Magic's `$000..$2ff` workspace.
    ///
    /// New bitmap tiles are editable only in slots 4 and 5, matching the recovered default
    /// allocation start at tile `$200`. Existing occupied tiles in slots 0–3 remain available for
    /// exact or flip-aware reuse. Tile `$0f8` is always protected as the recovered blank fallback.
    ///
    /// # Errors
    ///
    /// Rejects an assigned slot whose decoded graphics length is not exactly `$80` tiles.
    pub fn assemble(
        assignments: [Option<usize>; NATIVE_MAP16_BITMAP_SLOT_COUNT],
        slots: [Option<GraphicsFile4bpp>; NATIVE_MAP16_BITMAP_SLOT_COUNT],
    ) -> Result<Self, NativeMap16BitmapWorkspaceError> {
        let mut tiles = Vec::with_capacity(NATIVE_MAP16_BITMAP_TILE_COUNT);
        for (slot, graphics) in slots.into_iter().enumerate() {
            match graphics {
                Some(graphics) => {
                    if graphics.tiles.len() != NATIVE_MAP16_BITMAP_TILES_PER_SLOT {
                        return Err(NativeMap16BitmapWorkspaceError::WrongSlotTileCount {
                            slot,
                            actual: graphics.tiles.len(),
                        });
                    }
                    tiles.extend(graphics.tiles);
                }
                None => tiles.extend(
                    std::iter::repeat_with(|| IndexedTile::new([0; IndexedTile::PIXEL_COUNT]))
                        .take(NATIVE_MAP16_BITMAP_TILES_PER_SLOT),
                ),
            }
        }
        let occupied = tiles
            .iter()
            .map(|tile| tile.pixels().iter().any(|pixel| *pixel != 0))
            .collect::<Vec<_>>();
        let mut owners = vec![GraphicsTileOwner::Fixed; NATIVE_MAP16_BITMAP_ALLOCATION_START];
        owners.resize(NATIVE_MAP16_BITMAP_TILE_COUNT, GraphicsTileOwner::Editable);
        owners[NATIVE_MAP16_BITMAP_BLANK_TILE] = GraphicsTileOwner::Fixed;
        Ok(Self {
            assignments,
            graphics: GraphicsFile4bpp { tiles },
            ownership: GraphicsOwnership::from_owners(owners),
            occupied,
        })
    }

    /// Splits a staged workspace back into its six native slots.
    ///
    /// A changed blank-sentinel slot is rejected: without a concrete GFX/ExGFX assignment there is
    /// nowhere semantically valid to save those pixels in the ROM.
    ///
    /// # Errors
    ///
    /// Rejects a noncanonical workspace size or a modified unassigned slot.
    pub fn changed_assigned_slots(
        &self,
        staged: &GraphicsFile4bpp,
    ) -> Result<Vec<(usize, usize, GraphicsFile4bpp)>, NativeMap16BitmapWorkspaceError> {
        if staged.tiles.len() != NATIVE_MAP16_BITMAP_TILE_COUNT {
            return Err(NativeMap16BitmapWorkspaceError::WrongWorkspaceTileCount(
                staged.tiles.len(),
            ));
        }
        let mut changed = Vec::new();
        for slot in 0..NATIVE_MAP16_BITMAP_SLOT_COUNT {
            let start = slot * NATIVE_MAP16_BITMAP_TILES_PER_SLOT;
            let end = start + NATIVE_MAP16_BITMAP_TILES_PER_SLOT;
            if staged.tiles[start..end] == self.graphics.tiles[start..end] {
                continue;
            }
            let Some(file_number) = self.assignments[slot] else {
                return Err(NativeMap16BitmapWorkspaceError::ModifiedUnassignedSlot(
                    slot,
                ));
            };
            changed.push((
                slot,
                file_number,
                GraphicsFile4bpp {
                    tiles: staged.tiles[start..end].to_vec(),
                },
            ));
        }
        Ok(changed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeMap16BitmapWorkspaceError {
    WrongSlotTileCount { slot: usize, actual: usize },
    WrongWorkspaceTileCount(usize),
    ModifiedUnassignedSlot(usize),
}

impl fmt::Display for NativeMap16BitmapWorkspaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid native Map16 bitmap graphics workspace: {self:?}"
        )
    }
}

impl std::error::Error for NativeMap16BitmapWorkspaceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::IndexedBitmapImport;

    fn slot(pixel: u8) -> GraphicsFile4bpp {
        GraphicsFile4bpp {
            tiles: vec![
                IndexedTile::new([pixel; IndexedTile::PIXEL_COUNT]);
                NATIVE_MAP16_BITMAP_TILES_PER_SLOT
            ],
        }
    }

    #[test]
    fn assembles_six_slots_with_recovered_native_ownership_and_occupancy() {
        let workspace = NativeMap16BitmapGraphicsWorkspace::assemble(
            [Some(0x14), Some(0x17), Some(0x19), Some(0x15), None, None],
            [
                Some(slot(1)),
                Some(slot(2)),
                Some(slot(3)),
                Some(slot(4)),
                None,
                None,
            ],
        )
        .unwrap();
        assert_eq!(workspace.graphics.tiles.len(), 0x300);
        assert!(workspace.occupied[..0x200].iter().all(|occupied| *occupied));
        assert!(
            workspace.occupied[0x200..]
                .iter()
                .all(|occupied| !*occupied)
        );
        assert_eq!(
            workspace.ownership.owner(0x1ff),
            Some(GraphicsTileOwner::Fixed)
        );
        assert_eq!(
            workspace.ownership.owner(0x200),
            Some(GraphicsTileOwner::Editable)
        );
    }

    #[test]
    fn changed_slots_retain_assignments_and_refuse_sentinel_data_loss() {
        let workspace = NativeMap16BitmapGraphicsWorkspace::assemble(
            [Some(0), Some(1), Some(2), Some(3), Some(0x40), None],
            [
                Some(slot(1)),
                Some(slot(2)),
                Some(slot(3)),
                Some(slot(4)),
                Some(slot(0)),
                None,
            ],
        )
        .unwrap();
        let mut staged = workspace.graphics.clone();
        staged.tiles[0x200] = IndexedTile::new([7; IndexedTile::PIXEL_COUNT]);
        let changed = workspace.changed_assigned_slots(&staged).unwrap();
        assert_eq!(changed.len(), 1);
        assert_eq!((changed[0].0, changed[0].1), (4, 0x40));

        staged.tiles[0x280] = IndexedTile::new([8; IndexedTile::PIXEL_COUNT]);
        assert_eq!(
            workspace.changed_assigned_slots(&staged),
            Err(NativeMap16BitmapWorkspaceError::ModifiedUnassignedSlot(5))
        );
    }

    #[test]
    fn assigned_slots_have_exact_native_shape() {
        let error = NativeMap16BitmapGraphicsWorkspace::assemble(
            [Some(0), None, None, None, None, None],
            [
                Some(GraphicsFile4bpp {
                    tiles: vec![IndexedTile::new([0; 64]); 0x7f],
                }),
                None,
                None,
                None,
                None,
                None,
            ],
        )
        .unwrap_err();
        assert_eq!(
            error,
            NativeMap16BitmapWorkspaceError::WrongSlotTileCount {
                slot: 0,
                actual: 0x7f
            }
        );
    }

    #[test]
    fn bitmap_materialization_allocates_at_recovered_tile_200() {
        let workspace = NativeMap16BitmapGraphicsWorkspace::assemble(
            [Some(0), Some(1), Some(2), Some(3), Some(0x40), Some(0x41)],
            [
                Some(slot(1)),
                Some(slot(2)),
                Some(slot(3)),
                Some(slot(4)),
                Some(slot(0)),
                Some(slot(0)),
            ],
        )
        .unwrap();
        let imported = IndexedBitmapImport::materialize(
            8,
            8,
            &[7; IndexedTile::PIXEL_COUNT],
            &workspace.graphics,
            &workspace.ownership,
            &workspace.occupied,
        )
        .unwrap();
        assert_eq!(usize::from(imported.placements[0].tile), 0x200);
        assert!(imported.occupied[0x200]);
    }
}
