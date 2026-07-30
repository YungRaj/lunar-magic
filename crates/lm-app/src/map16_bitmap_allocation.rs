//! Lunar Magic-compatible placement of imported definitions in the global Map16 namespace.

use lm_level::Map16Tile;
use std::fmt;

/// Graphics word used by Lunar Magic for every quadrant of an unoccupied Map16 definition.
pub const LUNAR_MAGIC_BLANK_MAP16_WORD: u16 = 0x1004;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Map16BitmapAllocationMode {
    Sequential,
    Deduplicated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Map16BitmapAllocationOptions {
    /// First global Map16 index considered by `FindNextBlankMap16Tile`.
    pub start: usize,
    /// Exclusive global Map16 bound.
    pub end: usize,
    /// One index Lunar Magic refuses to allocate even when its definition is blank.
    pub reserved: usize,
    pub mode: Map16BitmapAllocationMode,
}

/// Result of applying as much of one import as Lunar Magic can place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map16BitmapAllocation {
    /// Global Map16 index for every consumed source definition.
    ///
    /// This is shorter than the source when the global blank-tile space is exhausted.
    pub assignments: Vec<usize>,
    /// Cursor value written by the last successful blank-tile search.
    pub next_cursor: usize,
    /// Number of globally allocated definitions. Deduplicated aliases are not counted.
    pub allocated_definitions: usize,
    /// True when placement stopped because no blank definition remained before `end`.
    pub exhausted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Map16BitmapAllocationError {
    InvertedRange { start: usize, end: usize },
    EndOutOfRange { end: usize, definitions: usize },
    ReservedSourceCount { sources: usize, imported: usize },
}

impl fmt::Display for Map16BitmapAllocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid bitmap Map16 allocation: {self:?}")
    }
}

impl std::error::Error for Map16BitmapAllocationError {}

/// Applies imported graphics definitions to Lunar Magic's global Map16 workspace.
///
/// This follows `FindNextBlankMap16Tile` at `004ef030` and its callers at `004ef090` and
/// `004ef2d0`: scan upward from a shared cursor, skip one reserved index, accept only four
/// `0x1004` graphics words, and retain a successfully imported prefix on exhaustion. Only the
/// four graphics words are replaced; the target definition's existing Acts-Like value survives.
///
/// Deduplication compares the four imported graphics words and reuses the first earlier assignment.
///
/// # Errors
///
/// Rejects inverted bounds or an upper bound beyond the supplied complete Map16 workspace without
/// changing any definition.
pub fn allocate_bitmap_map16_tiles(
    definitions: &mut [Map16Tile],
    imported: &[Map16Tile],
    options: Map16BitmapAllocationOptions,
) -> Result<Map16BitmapAllocation, Map16BitmapAllocationError> {
    allocate_bitmap_map16_tiles_with_reserved_sources(definitions, imported, &[], options)
}

/// Applies the same allocation while mapping selected source blocks directly to `reserved`.
///
/// Lunar Magic's deduplicated caller uses this for 16×16 source blocks whose four referenced 8×8
/// graphics tiles are all empty. Reserved-source mapping is ignored in sequential mode.
///
/// # Errors
///
/// Returns the same bound errors as [`allocate_bitmap_map16_tiles`] and rejects a nonempty
/// `reserved_sources` slice whose length differs from `imported`.
pub fn allocate_bitmap_map16_tiles_with_reserved_sources(
    definitions: &mut [Map16Tile],
    imported: &[Map16Tile],
    reserved_sources: &[bool],
    options: Map16BitmapAllocationOptions,
) -> Result<Map16BitmapAllocation, Map16BitmapAllocationError> {
    if options.start > options.end {
        return Err(Map16BitmapAllocationError::InvertedRange {
            start: options.start,
            end: options.end,
        });
    }
    if options.end > definitions.len() {
        return Err(Map16BitmapAllocationError::EndOutOfRange {
            end: options.end,
            definitions: definitions.len(),
        });
    }
    if !reserved_sources.is_empty() && reserved_sources.len() != imported.len() {
        return Err(Map16BitmapAllocationError::ReservedSourceCount {
            sources: reserved_sources.len(),
            imported: imported.len(),
        });
    }

    let mut cursor = options.start;
    let mut assignments = Vec::with_capacity(imported.len());
    let mut allocated_definitions = 0;
    for (source_index, source) in imported.iter().enumerate() {
        if options.mode == Map16BitmapAllocationMode::Deduplicated
            && reserved_sources.get(source_index) == Some(&true)
        {
            assignments.push(options.reserved);
            continue;
        }
        if options.mode == Map16BitmapAllocationMode::Deduplicated
            && let Some(previous) = imported[..source_index]
                .iter()
                .position(|candidate| same_graphics(*candidate, *source))
        {
            assignments.push(assignments[previous]);
            continue;
        }

        let Some(target) = find_next_blank(definitions, cursor, options.end, options.reserved)
        else {
            return Ok(Map16BitmapAllocation {
                assignments,
                next_cursor: options.end.saturating_add(1),
                allocated_definitions,
                exhausted: true,
            });
        };
        copy_graphics(&mut definitions[target], *source);
        assignments.push(target);
        allocated_definitions += 1;
        cursor = target + 1;
    }

    Ok(Map16BitmapAllocation {
        assignments,
        next_cursor: cursor,
        allocated_definitions,
        exhausted: false,
    })
}

#[must_use]
pub const fn is_lunar_magic_blank_map16_tile(tile: Map16Tile) -> bool {
    tile.top_left.0 == LUNAR_MAGIC_BLANK_MAP16_WORD
        && tile.top_right.0 == LUNAR_MAGIC_BLANK_MAP16_WORD
        && tile.bottom_left.0 == LUNAR_MAGIC_BLANK_MAP16_WORD
        && tile.bottom_right.0 == LUNAR_MAGIC_BLANK_MAP16_WORD
}

fn find_next_blank(
    definitions: &[Map16Tile],
    mut cursor: usize,
    end: usize,
    reserved: usize,
) -> Option<usize> {
    while cursor < end {
        if cursor != reserved && is_lunar_magic_blank_map16_tile(definitions[cursor]) {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

const fn same_graphics(left: Map16Tile, right: Map16Tile) -> bool {
    left.top_left.0 == right.top_left.0
        && left.top_right.0 == right.top_right.0
        && left.bottom_left.0 == right.bottom_left.0
        && left.bottom_right.0 == right.bottom_right.0
}

const fn copy_graphics(target: &mut Map16Tile, source: Map16Tile) {
    target.top_left = source.top_left;
    target.top_right = source.top_right;
    target.bottom_left = source.bottom_left;
    target.bottom_right = source.bottom_right;
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::Subtile;

    fn blank(acts_like: u16) -> Map16Tile {
        Map16Tile {
            top_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
            top_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
            bottom_left: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
            bottom_right: Subtile(LUNAR_MAGIC_BLANK_MAP16_WORD),
            acts_like,
        }
    }

    fn imported(word: u16, acts_like: u16) -> Map16Tile {
        Map16Tile {
            top_left: Subtile(word),
            top_right: Subtile(word + 1),
            bottom_left: Subtile(word + 2),
            bottom_right: Subtile(word + 3),
            acts_like,
        }
    }

    fn options(mode: Map16BitmapAllocationMode) -> Map16BitmapAllocationOptions {
        Map16BitmapAllocationOptions {
            start: 2,
            end: 8,
            reserved: 4,
            mode,
        }
    }

    #[test]
    fn sequential_scan_skips_occupied_and_reserved_tiles_and_preserves_acts_like() {
        let mut definitions = vec![blank(0); 8];
        definitions[2] = imported(0x20, 0x222);
        let source = [imported(0x100, 0x999), imported(0x200, 0xaaa)];
        let result = allocate_bitmap_map16_tiles(
            &mut definitions,
            &source,
            options(Map16BitmapAllocationMode::Sequential),
        )
        .unwrap();

        assert_eq!(result.assignments, [3, 5]);
        assert_eq!(result.next_cursor, 6);
        assert_eq!(result.allocated_definitions, 2);
        assert!(!result.exhausted);
        assert_eq!(definitions[3].top_left, source[0].top_left);
        assert_eq!(definitions[3].acts_like, 0);
        assert!(is_lunar_magic_blank_map16_tile(definitions[4]));
    }

    #[test]
    fn deduplication_ignores_source_acts_like_and_reuses_first_assignment() {
        let mut definitions = vec![blank(0x130); 8];
        let source = [imported(0x100, 1), imported(0x100, 2), imported(0x200, 3)];
        let result = allocate_bitmap_map16_tiles(
            &mut definitions,
            &source,
            options(Map16BitmapAllocationMode::Deduplicated),
        )
        .unwrap();

        assert_eq!(result.assignments, [2, 2, 3]);
        assert_eq!(result.allocated_definitions, 2);
        assert_eq!(definitions[2].acts_like, 0x130);
    }

    #[test]
    fn exhaustion_retains_the_imported_prefix_like_lunar_magic() {
        let mut definitions = vec![blank(0); 5];
        definitions[2] = imported(0x20, 0);
        let source = [imported(0x100, 0), imported(0x200, 0)];
        let mut allocation_options = options(Map16BitmapAllocationMode::Sequential);
        allocation_options.end = definitions.len();
        let result =
            allocate_bitmap_map16_tiles(&mut definitions, &source, allocation_options).unwrap();

        assert_eq!(result.assignments, [3]);
        assert_eq!(result.next_cursor, 6);
        assert_eq!(result.allocated_definitions, 1);
        assert!(result.exhausted);
        assert_eq!(definitions[3].top_left, source[0].top_left);
    }

    #[test]
    fn invalid_bounds_do_not_modify_the_workspace() {
        let mut definitions = vec![blank(0); 8];
        let before = definitions.clone();
        assert_eq!(
            allocate_bitmap_map16_tiles(
                &mut definitions,
                &[imported(0x100, 0)],
                Map16BitmapAllocationOptions {
                    start: 7,
                    end: 6,
                    reserved: 4,
                    mode: Map16BitmapAllocationMode::Sequential,
                },
            ),
            Err(Map16BitmapAllocationError::InvertedRange { start: 7, end: 6 })
        );
        assert_eq!(definitions, before);
    }

    #[test]
    fn sequential_assignments_cross_a_map16_page_boundary() {
        let mut definitions = vec![blank(0); 512];
        let source = [imported(0x100, 0), imported(0x200, 0)];
        let end = definitions.len();
        let result = allocate_bitmap_map16_tiles(
            &mut definitions,
            &source,
            Map16BitmapAllocationOptions {
                start: 255,
                end,
                reserved: usize::MAX,
                mode: Map16BitmapAllocationMode::Sequential,
            },
        )
        .unwrap();

        assert_eq!(result.assignments, [255, 256]);
        assert_eq!(definitions[255].top_left, source[0].top_left);
        assert_eq!(definitions[256].top_left, source[1].top_left);
    }

    #[test]
    fn deduplicated_empty_blocks_map_to_reserved_without_consuming_space() {
        let mut definitions = vec![blank(0); 8];
        let before = definitions.clone();
        let source = [imported(0x100, 0), imported(0x200, 0)];
        let result = allocate_bitmap_map16_tiles_with_reserved_sources(
            &mut definitions,
            &source,
            &[true, true],
            options(Map16BitmapAllocationMode::Deduplicated),
        )
        .unwrap();

        assert_eq!(result.assignments, [4, 4]);
        assert_eq!(result.next_cursor, 2);
        assert_eq!(result.allocated_definitions, 0);
        assert_eq!(definitions, before);
    }

    #[test]
    fn sequential_mode_materializes_empty_blocks_instead_of_using_reserved() {
        let mut definitions = vec![blank(0); 8];
        let source = [imported(0x100, 0)];
        let result = allocate_bitmap_map16_tiles_with_reserved_sources(
            &mut definitions,
            &source,
            &[true],
            options(Map16BitmapAllocationMode::Sequential),
        )
        .unwrap();

        assert_eq!(result.assignments, [2]);
        assert_eq!(result.allocated_definitions, 1);
        assert_eq!(definitions[2].top_left, source[0].top_left);
    }
}
