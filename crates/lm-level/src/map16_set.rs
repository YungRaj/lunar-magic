use crate::{BinaryError, Map16Page, Map16Tile};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Map16Set {
    pub pages: Vec<Map16Page>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActsLikeResolution {
    pub chain: Vec<u16>,
    pub terminal: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Map16SetError {
    PlaneSize { graphics: usize, acts_like: usize },
    TooManyPages(usize),
    WrongPageSize { page: usize, tiles: usize },
    Decode(BinaryError),
    TileOutOfRange(u16),
    ActsLikeOutOfRange { tile: u16, target: u16 },
    ActsLikeCycle { cycle: Vec<u16> },
    ResolutionLimit(usize),
}

impl fmt::Display for Map16SetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Map16 set: {self:?}")
    }
}

impl std::error::Error for Map16SetError {}

impl From<BinaryError> for Map16SetError {
    fn from(value: BinaryError) -> Self {
        Self::Decode(value)
    }
}

impl Map16Set {
    pub const MAX_PAGES: usize = 256;
    pub const GRAPHICS_PAGE_LEN: usize = Map16Page::TILE_COUNT * Map16Tile::GRAPHICS_LEN;
    pub const ACTS_LIKE_PAGE_LEN: usize = Map16Page::TILE_COUNT * 2;

    /// Validates the page count and exact 256-tile shape of every public page model.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetError`] for excessive pages or a malformed page length.
    pub fn validate_shape(&self) -> Result<(), Map16SetError> {
        if self.pages.len() > Self::MAX_PAGES {
            return Err(Map16SetError::TooManyPages(self.pages.len()));
        }
        for (page, value) in self.pages.iter().enumerate() {
            if value.tiles.len() != Map16Page::TILE_COUNT {
                return Err(Map16SetError::WrongPageSize {
                    page,
                    tiles: value.tiles.len(),
                });
            }
        }
        Ok(())
    }

    /// Decodes complete parallel graphics and Acts Like page planes.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetError`] unless both planes contain the same whole number of pages.
    pub fn decode(graphics: &[u8], acts_like: &[u8]) -> Result<Self, Map16SetError> {
        if graphics.len() % Self::GRAPHICS_PAGE_LEN != 0
            || acts_like.len() % Self::ACTS_LIKE_PAGE_LEN != 0
            || graphics.len() / Self::GRAPHICS_PAGE_LEN
                != acts_like.len() / Self::ACTS_LIKE_PAGE_LEN
        {
            return Err(Map16SetError::PlaneSize {
                graphics: graphics.len(),
                acts_like: acts_like.len(),
            });
        }
        let page_count = graphics.len() / Self::GRAPHICS_PAGE_LEN;
        if page_count > Self::MAX_PAGES {
            return Err(Map16SetError::TooManyPages(page_count));
        }
        let pages = graphics
            .chunks_exact(Self::GRAPHICS_PAGE_LEN)
            .zip(acts_like.chunks_exact(Self::ACTS_LIKE_PAGE_LEN))
            .map(|(graphics, acts_like)| Map16Page::decode(graphics, acts_like))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { pages })
    }

    /// Encodes complete parallel graphics and Acts Like planes.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetError`] unless every public page contains exactly 256 tiles and the set
    /// fits the 16-bit page namespace.
    pub fn encode(&self) -> Result<(Vec<u8>, Vec<u8>), Map16SetError> {
        self.validate_shape()?;
        let mut graphics = Vec::with_capacity(self.pages.len() * Self::GRAPHICS_PAGE_LEN);
        let mut acts_like = Vec::with_capacity(self.pages.len() * Self::ACTS_LIKE_PAGE_LEN);
        for (page_index, page) in self.pages.iter().enumerate() {
            let (page_graphics, page_acts_like) =
                page.encode()
                    .map_err(|error| Map16SetError::WrongPageSize {
                        page: page_index,
                        tiles: error.tiles,
                    })?;
            graphics.extend(page_graphics);
            acts_like.extend(page_acts_like);
        }
        Ok((graphics, acts_like))
    }

    #[must_use]
    pub fn tile(&self, tile: u16) -> Option<&Map16Tile> {
        let index = usize::from(tile);
        self.pages
            .get(index / Map16Page::TILE_COUNT)?
            .tiles
            .get(index % Map16Page::TILE_COUNT)
    }

    /// Resolves an Acts Like chain while accepting self-links as terminal definitions.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetError`] for missing tiles, multi-node cycles, or a chain beyond `limit`.
    pub fn resolve_acts_like(
        &self,
        tile: u16,
        limit: usize,
    ) -> Result<ActsLikeResolution, Map16SetError> {
        let mut positions = BTreeMap::new();
        let mut chain = Vec::new();
        let mut current = tile;
        for step in 0..limit {
            let definition = self.tile(current).ok_or(if step == 0 {
                Map16SetError::TileOutOfRange(current)
            } else {
                Map16SetError::ActsLikeOutOfRange {
                    tile: *chain.last().unwrap_or(&tile),
                    target: current,
                }
            })?;
            positions.insert(current, chain.len());
            chain.push(current);
            let next = definition.acts_like;
            if next == current {
                return Ok(ActsLikeResolution {
                    chain,
                    terminal: current,
                });
            }
            if let Some(&cycle_start) = positions.get(&next) {
                let mut cycle = chain[cycle_start..].to_vec();
                cycle.push(next);
                return Err(Map16SetError::ActsLikeCycle { cycle });
            }
            current = next;
        }
        Err(Map16SetError::ResolutionLimit(limit))
    }

    /// Resolves every tile's Acts Like chain and returns the first invalid link or cycle.
    ///
    /// # Errors
    ///
    /// Returns [`Map16SetError`] when the set exceeds the 16-bit tile namespace or any chain is
    /// out of range, cyclic, or longer than `limit`.
    pub fn validate_acts_like(&self, limit: usize) -> Result<(), Map16SetError> {
        self.validate_shape()?;
        let tile_count = self
            .pages
            .len()
            .checked_mul(Map16Page::TILE_COUNT)
            .ok_or(Map16SetError::ResolutionLimit(limit))?;
        let mut resolved_distance = vec![None; tile_count];
        for start in 0..tile_count {
            if resolved_distance[start].is_some() {
                continue;
            }
            let mut positions = BTreeMap::new();
            let mut chain = Vec::new();
            let mut current =
                u16::try_from(start).map_err(|_| Map16SetError::TileOutOfRange(u16::MAX))?;
            let terminal_distance = loop {
                let index = usize::from(current);
                if index >= tile_count {
                    return Err(Map16SetError::ActsLikeOutOfRange {
                        tile: *chain.last().unwrap_or(&current),
                        target: current,
                    });
                }
                if let Some(distance) = resolved_distance[index] {
                    if chain
                        .len()
                        .checked_add(distance)
                        .is_none_or(|length| length > limit)
                    {
                        return Err(Map16SetError::ResolutionLimit(limit));
                    }
                    break distance;
                }
                if chain.len() >= limit {
                    return Err(Map16SetError::ResolutionLimit(limit));
                }
                positions.insert(current, chain.len());
                chain.push(current);
                let next = self.pages[index / Map16Page::TILE_COUNT].tiles
                    [index % Map16Page::TILE_COUNT]
                    .acts_like;
                if next == current {
                    break 0;
                }
                if let Some(&cycle_start) = positions.get(&next) {
                    let mut cycle = chain[cycle_start..].to_vec();
                    cycle.push(next);
                    return Err(Map16SetError::ActsLikeCycle { cycle });
                }
                current = next;
            };
            let mut distance = terminal_distance;
            for tile in chain.into_iter().rev() {
                distance = distance
                    .checked_add(1)
                    .ok_or(Map16SetError::ResolutionLimit(limit))?;
                resolved_distance[usize::from(tile)] = Some(distance);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Subtile;

    fn set() -> Map16Set {
        let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT * 2];
        for (index, tile) in tiles.iter_mut().enumerate() {
            tile.acts_like = u16::try_from(index).unwrap();
            tile.top_left = Subtile(u16::try_from(index).unwrap());
        }
        Map16Set {
            pages: tiles
                .chunks_exact(Map16Page::TILE_COUNT)
                .map(|page| Map16Page::new(page.to_vec()).unwrap())
                .collect(),
        }
    }

    #[test]
    fn complete_parallel_planes_round_trip() {
        let expected = set();
        let (graphics, acts_like) = expected.encode().unwrap();
        assert_eq!(graphics.len(), Map16Set::GRAPHICS_PAGE_LEN * 2);
        assert_eq!(acts_like.len(), Map16Set::ACTS_LIKE_PAGE_LEN * 2);
        assert_eq!(Map16Set::decode(&graphics, &acts_like).unwrap(), expected);
        assert!(Map16Set::decode(&graphics[..graphics.len() - 1], &acts_like).is_err());
    }

    #[test]
    fn acts_like_chains_resolve_across_pages() {
        let mut set = set();
        set.pages[0].tiles[1].acts_like = 0x100;
        set.pages[1].tiles[0].acts_like = 0x101;
        assert_eq!(
            set.resolve_acts_like(1, 8).unwrap(),
            ActsLikeResolution {
                chain: vec![1, 0x100, 0x101],
                terminal: 0x101,
            }
        );
    }

    #[test]
    fn multi_node_cycles_and_missing_targets_are_distinct() {
        let mut set = set();
        set.pages[0].tiles[2].acts_like = 3;
        set.pages[0].tiles[3].acts_like = 2;
        assert_eq!(
            set.resolve_acts_like(2, 8),
            Err(Map16SetError::ActsLikeCycle {
                cycle: vec![2, 3, 2]
            })
        );
        set.pages[0].tiles[4].acts_like = 0x300;
        assert!(matches!(
            set.resolve_acts_like(4, 8),
            Err(Map16SetError::ActsLikeOutOfRange {
                tile: 4,
                target: 0x300
            })
        ));
    }

    #[test]
    fn whole_set_validation_checks_every_chain() {
        let mut set = set();
        set.validate_acts_like(8).unwrap();
        set.pages[1].tiles[2].acts_like = 0x300;
        assert!(matches!(
            set.validate_acts_like(8),
            Err(Map16SetError::ActsLikeOutOfRange {
                tile: 0x102,
                target: 0x300,
            })
        ));
    }

    #[test]
    fn whole_set_limit_counts_the_cached_suffix_without_iteration_order_dependence() {
        let mut set = set();
        set.pages[0].tiles[1].acts_like = 0;
        assert_eq!(
            set.resolve_acts_like(1, 1),
            Err(Map16SetError::ResolutionLimit(1))
        );
        assert_eq!(
            set.validate_acts_like(1),
            Err(Map16SetError::ResolutionLimit(1))
        );
        assert_eq!(set.resolve_acts_like(1, 2).unwrap().chain, vec![1, 0]);
        set.validate_acts_like(2).unwrap();

        // Reverse the edge so the longer chain is encountered before its suffix; the result is
        // identical and therefore independent of numeric tile iteration order.
        set.pages[0].tiles[1].acts_like = 1;
        set.pages[0].tiles[0].acts_like = 1;
        assert_eq!(
            set.validate_acts_like(1),
            Err(Map16SetError::ResolutionLimit(1))
        );
        set.validate_acts_like(2).unwrap();
    }

    #[test]
    fn cached_whole_set_validation_matches_individual_resolution_for_small_graphs() {
        const NODES: usize = 4;
        let graph_count = NODES.pow(u32::try_from(NODES).unwrap());
        for mut encoded_graph in 0..graph_count {
            let mut set = Map16Set {
                pages: vec![
                    Map16Page::new(
                        (0..Map16Page::TILE_COUNT)
                            .map(|index| Map16Tile {
                                acts_like: u16::try_from(index).unwrap(),
                                ..Map16Tile::default()
                            })
                            .collect(),
                    )
                    .unwrap(),
                ],
            };
            for tile in 0..NODES {
                set.pages[0].tiles[tile].acts_like = u16::try_from(encoded_graph % NODES).unwrap();
                encoded_graph /= NODES;
            }
            for limit in 0..=NODES + 1 {
                let individually_valid = (0..Map16Page::TILE_COUNT).all(|tile| {
                    set.resolve_acts_like(u16::try_from(tile).unwrap(), limit)
                        .is_ok()
                });
                assert_eq!(
                    set.validate_acts_like(limit).is_ok(),
                    individually_valid,
                    "limit {limit}, graph {:?}",
                    set.pages[0].tiles[..NODES]
                        .iter()
                        .map(|tile| tile.acts_like)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn decode_rejects_more_than_the_sixteen_bit_tile_namespace() {
        let graphics = vec![0; Map16Set::GRAPHICS_PAGE_LEN * (Map16Set::MAX_PAGES + 1)];
        let acts_like = vec![0; Map16Set::ACTS_LIKE_PAGE_LEN * (Map16Set::MAX_PAGES + 1)];
        assert_eq!(
            Map16Set::decode(&graphics, &acts_like),
            Err(Map16SetError::TooManyPages(257))
        );
    }

    #[test]
    fn public_malformed_page_shape_is_reported_before_graph_indexing() {
        let malformed = Map16Set {
            pages: vec![Map16Page { tiles: vec![] }],
        };
        assert_eq!(
            malformed.validate_acts_like(257),
            Err(Map16SetError::WrongPageSize { page: 0, tiles: 0 })
        );
        assert_eq!(
            malformed.encode(),
            Err(Map16SetError::WrongPageSize { page: 0, tiles: 0 })
        );
    }
}
