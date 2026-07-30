use crate::{Map16Page, Map16Set, Map16SetError, Map16Tile, Subtile};
use std::{collections::BTreeSet, fmt};

/// One tile location in the complete Map16 workspace.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Map16Address {
    pub page: usize,
    pub tile: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Map16Quadrant {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16EditError {
    PageOutOfRange { page: usize, len: usize },
    TileOutOfRange { tile: usize },
    DuplicateTarget(Map16Address),
    MalformedPage { page: usize, tiles: usize },
    TooManyPages(usize),
    EmptySet,
    SubtileNumberOutOfRange(u16),
    PaletteOutOfRange(u8),
    BackgroundActsLike(Map16Address),
    ActsLike(Map16SetError),
}

impl fmt::Display for Map16EditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Map16 edit: {self:?}")
    }
}

impl std::error::Error for Map16EditError {}

impl From<Map16SetError> for Map16EditError {
    fn from(error: Map16SetError) -> Self {
        Self::ActsLike(error)
    }
}

impl Subtile {
    /// Replaces the ten-bit graphics tile number while preserving attributes.
    ///
    /// # Errors
    ///
    /// Returns [`Map16EditError::SubtileNumberOutOfRange`] above `0x3ff`.
    pub fn set_tile_number(&mut self, tile_number: u16) -> Result<(), Map16EditError> {
        if tile_number > 0x03ff {
            return Err(Map16EditError::SubtileNumberOutOfRange(tile_number));
        }
        self.0 = (self.0 & !0x03ff) | tile_number;
        Ok(())
    }

    /// Replaces the three-bit palette row while preserving all other fields.
    ///
    /// # Errors
    ///
    /// Returns [`Map16EditError::PaletteOutOfRange`] above seven.
    pub fn set_palette(&mut self, palette: u8) -> Result<(), Map16EditError> {
        if palette > 7 {
            return Err(Map16EditError::PaletteOutOfRange(palette));
        }
        self.0 = (self.0 & !(7 << 10)) | (u16::from(palette) << 10);
        Ok(())
    }

    pub fn set_priority(&mut self, priority: bool) {
        set_flag(&mut self.0, 0x2000, priority);
    }

    pub fn set_x_flip(&mut self, flipped: bool) {
        set_flag(&mut self.0, 0x4000, flipped);
    }

    pub fn set_y_flip(&mut self, flipped: bool) {
        set_flag(&mut self.0, 0x8000, flipped);
    }
}

impl Map16Set {
    /// Atomically replaces unique tile targets and validates the complete Acts Like graph.
    ///
    /// An empty edit list is a no-op. Every page shape and target is checked before cloning or
    /// changing the set. `resolution_limit` bounds graph traversal for hostile workspaces.
    ///
    /// # Errors
    ///
    /// Returns [`Map16EditError`] for malformed pages, invalid/duplicate targets, or an invalid
    /// resulting Acts Like graph. Failure leaves the set unchanged.
    pub fn replace_tiles(
        &mut self,
        replacements: &[(Map16Address, Map16Tile)],
        resolution_limit: usize,
    ) -> Result<(), Map16EditError> {
        self.validate_edit_shape()?;
        let mut targets = BTreeSet::new();
        for (address, _) in replacements {
            self.validate_address(*address)?;
            if !targets.insert(*address) {
                return Err(Map16EditError::DuplicateTarget(*address));
            }
        }
        if replacements.is_empty() {
            return Ok(());
        }
        let mut staged = self.clone();
        for (address, tile) in replacements {
            staged.pages[address.page].tiles[address.tile] = *tile;
        }
        staged.validate_acts_like(resolution_limit)?;
        *self = staged;
        Ok(())
    }

    /// Changes one quadrant through the same atomic graph-validated edit path.
    ///
    /// # Errors
    ///
    /// Returns [`Map16EditError`] when the address/workspace is invalid.
    pub fn set_subtile(
        &mut self,
        address: Map16Address,
        quadrant: Map16Quadrant,
        subtile: Subtile,
        resolution_limit: usize,
    ) -> Result<(), Map16EditError> {
        self.validate_edit_shape()?;
        self.validate_address(address)?;
        let mut tile = self.pages[address.page].tiles[address.tile];
        match quadrant {
            Map16Quadrant::TopLeft => tile.top_left = subtile,
            Map16Quadrant::TopRight => tile.top_right = subtile,
            Map16Quadrant::BottomLeft => tile.bottom_left = subtile,
            Map16Quadrant::BottomRight => tile.bottom_right = subtile,
        }
        self.replace_tiles(&[(address, tile)], resolution_limit)
    }

    /// Changes one Acts Like target and rejects a missing target or multi-node cycle atomically.
    ///
    /// # Errors
    ///
    /// Returns [`Map16EditError`] when the address or resulting graph is invalid.
    pub fn set_acts_like(
        &mut self,
        address: Map16Address,
        acts_like: u16,
        resolution_limit: usize,
    ) -> Result<(), Map16EditError> {
        self.validate_edit_shape()?;
        self.validate_address(address)?;
        let mut tile = self.pages[address.page].tiles[address.tile];
        tile.acts_like = acts_like;
        self.replace_tiles(&[(address, tile)], resolution_limit)
    }

    /// Appends a complete page without renumbering existing tile IDs.
    ///
    /// # Errors
    ///
    /// Returns [`Map16EditError`] for a malformed page, namespace overflow, or invalid graph.
    pub fn push_page(
        &mut self,
        page: Map16Page,
        resolution_limit: usize,
    ) -> Result<(), Map16EditError> {
        self.validate_edit_shape()?;
        if page.tiles.len() != Map16Page::TILE_COUNT {
            return Err(Map16EditError::MalformedPage {
                page: self.pages.len(),
                tiles: page.tiles.len(),
            });
        }
        let count = self.pages.len().saturating_add(1);
        if count > Self::MAX_PAGES {
            return Err(Map16EditError::TooManyPages(count));
        }
        let mut staged = self.clone();
        staged.pages.push(page);
        staged.validate_acts_like(resolution_limit)?;
        *self = staged;
        Ok(())
    }

    /// Removes the last page without renumbering retained tile IDs.
    ///
    /// # Errors
    ///
    /// Returns [`Map16EditError::EmptySet`] for an empty set, or rejects removal when a retained
    /// Acts Like link would point into the removed page. Failure leaves the set unchanged.
    pub fn pop_page(&mut self, resolution_limit: usize) -> Result<Map16Page, Map16EditError> {
        self.validate_edit_shape()?;
        if self.pages.is_empty() {
            return Err(Map16EditError::EmptySet);
        }
        let mut staged = self.clone();
        let page = staged.pages.pop().ok_or(Map16EditError::EmptySet)?;
        staged.validate_acts_like(resolution_limit)?;
        *self = staged;
        Ok(page)
    }

    fn validate_edit_shape(&self) -> Result<(), Map16EditError> {
        if self.pages.len() > Self::MAX_PAGES {
            return Err(Map16EditError::TooManyPages(self.pages.len()));
        }
        for (page, value) in self.pages.iter().enumerate() {
            if value.tiles.len() != Map16Page::TILE_COUNT {
                return Err(Map16EditError::MalformedPage {
                    page,
                    tiles: value.tiles.len(),
                });
            }
        }
        Ok(())
    }

    fn validate_address(&self, address: Map16Address) -> Result<(), Map16EditError> {
        if address.page >= self.pages.len() {
            return Err(Map16EditError::PageOutOfRange {
                page: address.page,
                len: self.pages.len(),
            });
        }
        if address.tile >= Map16Page::TILE_COUNT {
            return Err(Map16EditError::TileOutOfRange { tile: address.tile });
        }
        Ok(())
    }
}

fn set_flag(word: &mut u16, mask: u16, enabled: bool) {
    if enabled {
        *word |= mask;
    } else {
        *word &= !mask;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Map16SetFile;

    fn page(base: u16) -> Map16Page {
        Map16Page::new(
            (0..Map16Page::TILE_COUNT)
                .map(|index| {
                    let id = base + u16::try_from(index).unwrap();
                    Map16Tile {
                        top_left: Subtile(id & 0x03ff),
                        acts_like: id,
                        ..Map16Tile::default()
                    }
                })
                .collect(),
        )
        .unwrap()
    }

    fn set() -> Map16Set {
        Map16Set {
            pages: vec![page(0), page(0x100)],
        }
    }

    #[test]
    fn packed_subtile_fields_preserve_each_other_and_reject_ranges() {
        let mut subtile = Subtile(0xffff);
        subtile.set_tile_number(0x123).unwrap();
        assert_eq!(subtile.tile_number(), 0x123);
        assert_eq!(subtile.palette(), 7);
        subtile.set_palette(2).unwrap();
        subtile.set_priority(false);
        subtile.set_x_flip(false);
        subtile.set_y_flip(true);
        assert_eq!(subtile.palette(), 2);
        assert!(!subtile.priority());
        assert!(!subtile.x_flip());
        assert!(subtile.y_flip());
        let unchanged = subtile;
        assert_eq!(
            subtile.set_tile_number(0x400),
            Err(Map16EditError::SubtileNumberOutOfRange(0x400))
        );
        assert_eq!(
            subtile.set_palette(8),
            Err(Map16EditError::PaletteOutOfRange(8))
        );
        assert_eq!(subtile, unchanged);
    }

    #[test]
    fn multi_tile_replace_is_atomic_and_rejects_duplicate_targets() {
        let mut set = set();
        let original = set.clone();
        let address = Map16Address { page: 0, tile: 1 };
        let mut first = set.pages[0].tiles[1];
        first.top_left = Subtile(9);
        assert_eq!(
            set.replace_tiles(&[(address, first), (address, first)], 0x200),
            Err(Map16EditError::DuplicateTarget(address))
        );
        assert_eq!(set, original);

        let cyclic = [
            (
                Map16Address { page: 0, tile: 1 },
                Map16Tile {
                    acts_like: 2,
                    ..first
                },
            ),
            (
                Map16Address { page: 0, tile: 2 },
                Map16Tile {
                    acts_like: 1,
                    ..set.pages[0].tiles[2]
                },
            ),
        ];
        assert!(matches!(
            set.replace_tiles(&cyclic, 0x200),
            Err(Map16EditError::ActsLike(
                Map16SetError::ActsLikeCycle { .. }
            ))
        ));
        assert_eq!(set, original);
    }

    #[test]
    fn valid_tile_and_quadrant_changes_commit_together() {
        let mut set = set();
        set.set_acts_like(Map16Address { page: 0, tile: 1 }, 0x100, 0x200)
            .unwrap();
        set.set_subtile(
            Map16Address { page: 1, tile: 0 },
            Map16Quadrant::BottomRight,
            Subtile(0xabcd),
            0x200,
        )
        .unwrap();
        assert_eq!(set.pages[0].tiles[1].acts_like, 0x100);
        assert_eq!(set.pages[1].tiles[0].bottom_right, Subtile(0xabcd));
    }

    #[test]
    fn page_removal_rejects_dangling_links_and_append_is_bounded() {
        let mut set = set();
        set.pages[0].tiles[1].acts_like = 0x100;
        let original = set.clone();
        assert!(matches!(
            set.pop_page(0x200),
            Err(Map16EditError::ActsLike(
                Map16SetError::ActsLikeOutOfRange { .. }
            ))
        ));
        assert_eq!(set, original);
        set.pages[0].tiles[1].acts_like = 1;
        assert_eq!(set.pop_page(0x200).unwrap().tiles.len(), 256);
        set.push_page(page(0x100), 0x200).unwrap();
        assert_eq!(set.pages.len(), 2);
    }

    #[test]
    fn malformed_public_pages_fail_without_mutation() {
        let mut set = Map16Set {
            pages: vec![Map16Page { tiles: vec![] }],
        };
        let original = set.clone();
        assert_eq!(
            set.replace_tiles(&[], 10),
            Err(Map16EditError::MalformedPage { page: 0, tiles: 0 })
        );
        assert_eq!(set, original);
    }

    #[test]
    fn semantic_edits_round_trip_through_complete_set_serialization() {
        let mut set = set();
        let address = Map16Address { page: 0, tile: 4 };
        let mut tile = set.pages[0].tiles[4];
        tile.top_left = Subtile(0xd234);
        tile.top_right = Subtile(0x4321);
        set.replace_tiles(&[(address, tile)], 0x200).unwrap();

        let bytes = Map16SetFile { set: set.clone() }.encode().unwrap();
        assert_eq!(Map16SetFile::decode(&bytes).unwrap().set, set);
    }
}
