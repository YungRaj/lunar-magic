use crate::{LevelObjectData, ObjectStreamError};
use std::collections::VecDeque;
use std::fmt;

pub const NATIVE_LAYER2_TILEMAP_LEN: usize = 0x800;
pub const LEGACY_LAYER2_TILEMAP_LEN: usize = 0x360;
pub const NATIVE_LAYER2_TILEMAP_WIDTH: usize = 32;
pub const NATIVE_LAYER2_TILEMAP_HEIGHT: usize = 32;

/// Maps 32×32 background-canvas coordinates to Lunar Magic's two column-major 32×16 planes.
#[must_use]
pub const fn native_layer2_tilemap_index(x: usize, y: usize) -> Option<usize> {
    if x >= NATIVE_LAYER2_TILEMAP_WIDTH || y >= NATIVE_LAYER2_TILEMAP_HEIGHT {
        return None;
    }
    Some(((y >> 4) * 31 + x) * 16 + y)
}

/// Returns the four-connected canvas region whose complete 16-bit words match the start cell.
///
/// The returned storage indexes are deterministic visual row-major order even though native
/// storage consists of two column-major 32×16 planes.
///
/// # Errors
///
/// Returns [`NativeLayer2Error`] unless `bytes` is one complete native tilemap and the start
/// coordinate lies inside its 32×32 canvas.
pub fn native_layer2_flood_region(
    bytes: &[u8],
    start_x: usize,
    start_y: usize,
) -> Result<Vec<usize>, NativeLayer2Error> {
    let visited = native_layer2_flood_mask(bytes, start_x, start_y)?;
    Ok(visited
        .iter()
        .enumerate()
        .filter(|(_, selected)| **selected)
        .map(|(visual, _)| native_layer2_storage_index_from_visual(visual))
        .collect())
}

/// Builds the edits for a repeating rectangular pattern over a matching flood region.
///
/// Pattern words are supplied in visual row-major order. The repeat origin is the minimum X/Y
/// corner of the connected region, matching Lunar Magic, and every result is normalized to a
/// 12-bit Map16 index.
///
/// # Errors
///
/// Returns [`NativeLayer2Error`] for malformed tilemap storage, an out-of-range start coordinate,
/// an empty pattern, mismatched pattern dimensions, or a pattern larger than the 32×32 canvas.
pub fn native_layer2_flood_pattern(
    bytes: &[u8],
    start_x: usize,
    start_y: usize,
    pattern_width: usize,
    pattern_height: usize,
    pattern: &[u16],
) -> Result<Vec<(usize, u16)>, NativeLayer2Error> {
    let expected = pattern_width.checked_mul(pattern_height);
    if pattern_width == 0
        || pattern_height == 0
        || pattern_width > NATIVE_LAYER2_TILEMAP_WIDTH
        || pattern_height > NATIVE_LAYER2_TILEMAP_HEIGHT
        || expected != Some(pattern.len())
    {
        return Err(NativeLayer2Error::FloodPatternShape {
            width: pattern_width,
            height: pattern_height,
            words: pattern.len(),
        });
    }
    let visited = native_layer2_flood_mask(bytes, start_x, start_y)?;
    let minimum_y = visited
        .iter()
        .position(|selected| *selected)
        .map_or(start_y, |visual| visual / NATIVE_LAYER2_TILEMAP_WIDTH);
    let minimum_x = visited
        .iter()
        .enumerate()
        .filter(|(_, selected)| **selected)
        .map(|(visual, _)| visual % NATIVE_LAYER2_TILEMAP_WIDTH)
        .min()
        .unwrap_or(start_x);
    let mut edits = Vec::new();
    for (visual, _) in visited
        .iter()
        .enumerate()
        .filter(|(_, selected)| **selected)
    {
        let x = visual % NATIVE_LAYER2_TILEMAP_WIDTH;
        let y = visual / NATIVE_LAYER2_TILEMAP_WIDTH;
        let pattern_x = (x - minimum_x) % pattern_width;
        let pattern_y = (y - minimum_y) % pattern_height;
        let pattern_index = pattern_y * pattern_width + pattern_x;
        let word =
            pattern
                .get(pattern_index)
                .copied()
                .ok_or(NativeLayer2Error::FloodPatternShape {
                    width: pattern_width,
                    height: pattern_height,
                    words: pattern.len(),
                })?;
        edits.push((
            native_layer2_storage_index_from_visual(visual),
            word & 0x0fff,
        ));
    }
    Ok(edits)
}

/// Builds the final-state edits for moving one rectangular selection by a whole-cell delta.
///
/// The source rectangle is snapshotted first, every source cell is cleared to `$0000`, and the
/// snapshot is then placed at the destination. Consequently, overlapping moves retain all selected
/// words and destination cells take precedence over source clearing.
///
/// # Errors
///
/// Returns [`NativeLayer2Error`] for malformed tilemap storage, an empty or out-of-range source
/// rectangle, or a destination rectangle outside the 32×32 canvas.
pub fn native_layer2_move_rectangle(
    bytes: &[u8],
    source_x: usize,
    source_y: usize,
    width: usize,
    height: usize,
    delta_x: i32,
    delta_y: i32,
) -> Result<Vec<(usize, u16)>, NativeLayer2Error> {
    if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
        return Err(NativeLayer2Error::TilemapLength(bytes.len()));
    }
    let source_end_x = source_x.checked_add(width);
    let source_end_y = source_y.checked_add(height);
    if width == 0
        || height == 0
        || source_end_x.is_none_or(|end| end > NATIVE_LAYER2_TILEMAP_WIDTH)
        || source_end_y.is_none_or(|end| end > NATIVE_LAYER2_TILEMAP_HEIGHT)
    {
        return Err(NativeLayer2Error::TilemapRectangle {
            x: source_x,
            y: source_y,
            width,
            height,
        });
    }
    let destination_x = i64::try_from(source_x)
        .ok()
        .and_then(|x| x.checked_add(i64::from(delta_x)));
    let destination_y = i64::try_from(source_y)
        .ok()
        .and_then(|y| y.checked_add(i64::from(delta_y)));
    let valid_destination = destination_x.zip(destination_y).and_then(|(x, y)| {
        let x = usize::try_from(x).ok()?;
        let y = usize::try_from(y).ok()?;
        let end_x = x.checked_add(width)?;
        let end_y = y.checked_add(height)?;
        (end_x <= NATIVE_LAYER2_TILEMAP_WIDTH && end_y <= NATIVE_LAYER2_TILEMAP_HEIGHT)
            .then_some((x, y))
    });
    let Some((destination_x, destination_y)) = valid_destination else {
        return Err(NativeLayer2Error::TilemapMove {
            x: source_x,
            y: source_y,
            width,
            height,
            delta_x,
            delta_y,
        });
    };

    let mut original = [0_u16; NATIVE_LAYER2_TILEMAP_WIDTH * NATIVE_LAYER2_TILEMAP_HEIGHT];
    for (visual, word) in original.iter_mut().enumerate() {
        *word = tilemap_word(bytes, native_layer2_storage_index_from_visual(visual));
    }
    let mut moved = original;
    let mut snapshot = Vec::with_capacity(width * height);
    for y in source_y..source_y + height {
        for x in source_x..source_x + width {
            snapshot.push(original[y * NATIVE_LAYER2_TILEMAP_WIDTH + x]);
            moved[y * NATIVE_LAYER2_TILEMAP_WIDTH + x] = 0;
        }
    }
    for (offset, word) in snapshot.into_iter().enumerate() {
        let x = destination_x + offset % width;
        let y = destination_y + offset / width;
        moved[y * NATIVE_LAYER2_TILEMAP_WIDTH + x] = word;
    }
    Ok(original
        .iter()
        .zip(moved)
        .enumerate()
        .filter(|(_, (before, after))| **before != *after)
        .map(|(visual, (_, word))| (native_layer2_storage_index_from_visual(visual), word))
        .collect())
}

fn native_layer2_flood_mask(
    bytes: &[u8],
    start_x: usize,
    start_y: usize,
) -> Result<[bool; NATIVE_LAYER2_TILEMAP_WIDTH * NATIVE_LAYER2_TILEMAP_HEIGHT], NativeLayer2Error> {
    if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
        return Err(NativeLayer2Error::TilemapLength(bytes.len()));
    }
    let start = native_layer2_tilemap_index(start_x, start_y).ok_or(
        NativeLayer2Error::TilemapCoordinate {
            x: start_x,
            y: start_y,
        },
    )?;
    let target = tilemap_word(bytes, start);
    let mut visited = [false; NATIVE_LAYER2_TILEMAP_WIDTH * NATIVE_LAYER2_TILEMAP_HEIGHT];
    let mut pending = VecDeque::from([(start_x, start_y)]);
    while let Some((x, y)) = pending.pop_front() {
        let visual = y * NATIVE_LAYER2_TILEMAP_WIDTH + x;
        if visited[visual] {
            continue;
        }
        let index = ((y >> 4) * 31 + x) * 16 + y;
        if tilemap_word(bytes, index) != target {
            continue;
        }
        visited[visual] = true;
        if x > 0 {
            pending.push_back((x - 1, y));
        }
        if x + 1 < NATIVE_LAYER2_TILEMAP_WIDTH {
            pending.push_back((x + 1, y));
        }
        if y > 0 {
            pending.push_back((x, y - 1));
        }
        if y + 1 < NATIVE_LAYER2_TILEMAP_HEIGHT {
            pending.push_back((x, y + 1));
        }
    }
    Ok(visited)
}

const fn native_layer2_storage_index_from_visual(visual: usize) -> usize {
    let x = visual % NATIVE_LAYER2_TILEMAP_WIDTH;
    let y = visual / NATIVE_LAYER2_TILEMAP_WIDTH;
    ((y >> 4) * 31 + x) * 16 + y
}

fn tilemap_word(bytes: &[u8], index: usize) -> u16 {
    let offset = index * 2;
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

/// Decoded native Layer 2 data selected by the level-mode storage class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLayer2Data {
    Objects(LevelObjectData),
    Tilemap(Vec<u8>),
}

/// Expands the legacy 0x360-byte tile-number layout into 0x400 little-endian tile words.
///
/// # Errors
///
/// Rejects input of any other length.
pub fn expand_legacy_layer2_tilemap(
    bytes: &[u8],
    high_byte: u8,
) -> Result<Vec<u8>, NativeLayer2Error> {
    if bytes.len() != LEGACY_LAYER2_TILEMAP_LEN {
        return Err(NativeLayer2Error::CompressedTilemapLength(bytes.len()));
    }
    let mut output = vec![0; NATIVE_LAYER2_TILEMAP_LEN];
    for (source, value) in bytes.iter().copied().enumerate() {
        let tile = if source < 0x1b0 {
            source
        } else {
            source + 0x50
        };
        output[tile * 2] = value;
        output[tile * 2 + 1] = high_byte;
    }
    Ok(output)
}

/// Compacts the legacy-representable portion of an interleaved tilemap.
///
/// # Errors
///
/// Rejects nonzero unused words or high bytes that differ from `high_byte`.
pub fn compact_legacy_layer2_tilemap(
    bytes: &[u8],
    high_byte: u8,
) -> Result<Vec<u8>, NativeLayer2Error> {
    if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
        return Err(NativeLayer2Error::TilemapLength(bytes.len()));
    }
    let mut output = Vec::with_capacity(LEGACY_LAYER2_TILEMAP_LEN);
    for tile in 0..0x400 {
        let represented = tile < 0x1b0 || (0x200..0x3b0).contains(&tile);
        let word = &bytes[tile * 2..tile * 2 + 2];
        if represented {
            if word[1] != high_byte {
                return Err(NativeLayer2Error::LegacyHighByte {
                    tile,
                    actual: word[1],
                    expected: high_byte,
                });
            }
            output.push(word[0]);
        } else if word != [0, 0] {
            return Err(NativeLayer2Error::LegacyUnusedWord { tile });
        }
    }
    Ok(output)
}

/// Interleaves two 0x400-byte low/high planes into 0x400 little-endian tile words.
///
/// # Errors
///
/// Rejects input of any other length.
pub fn interleave_layer2_tilemap_planes(bytes: &[u8]) -> Result<Vec<u8>, NativeLayer2Error> {
    if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
        return Err(NativeLayer2Error::CompressedTilemapLength(bytes.len()));
    }
    let (low, high) = bytes.split_at(0x400);
    let mut output = Vec::with_capacity(NATIVE_LAYER2_TILEMAP_LEN);
    for index in 0..0x400 {
        output.extend_from_slice(&[low[index], high[index]]);
    }
    Ok(output)
}

/// Splits 0x400 little-endian tile words into the low plane followed by the high plane.
///
/// # Errors
///
/// Rejects input of any other length.
pub fn split_layer2_tilemap_planes(bytes: &[u8]) -> Result<Vec<u8>, NativeLayer2Error> {
    if bytes.len() != NATIVE_LAYER2_TILEMAP_LEN {
        return Err(NativeLayer2Error::TilemapLength(bytes.len()));
    }
    let mut output = vec![0; NATIVE_LAYER2_TILEMAP_LEN];
    for (index, word) in bytes.chunks_exact(2).enumerate() {
        output[index] = word[0];
        output[0x400 + index] = word[1];
    }
    Ok(output)
}

impl NativeLayer2Data {
    /// Decodes the representation exported in an MWL Layer 2 payload.
    ///
    /// # Errors
    ///
    /// Rejects malformed object data or a tilemap that is not exactly 0x800 bytes.
    pub fn decode_mwl(level_mode: u8, bytes: &[u8]) -> Result<Self, NativeLayer2Error> {
        if level_mode_layer2_storage(level_mode) == Layer2Storage::Objects {
            Ok(Self::Objects(LevelObjectData::parse(bytes)?))
        } else if bytes.len() == NATIVE_LAYER2_TILEMAP_LEN {
            Ok(Self::Tilemap(bytes.to_vec()))
        } else {
            Err(NativeLayer2Error::TilemapLength(bytes.len()))
        }
    }

    /// Encodes the decoded MWL Layer 2 payload without its common metadata prefix.
    ///
    /// # Errors
    ///
    /// Rejects malformed object encoding or a non-0x800-byte tilemap.
    pub fn encode_mwl(&self) -> Result<Vec<u8>, NativeLayer2Error> {
        match self {
            Self::Objects(objects) => Ok(objects.encode_banked()?),
            Self::Tilemap(bytes) if bytes.len() == NATIVE_LAYER2_TILEMAP_LEN => Ok(bytes.clone()),
            Self::Tilemap(bytes) => Err(NativeLayer2Error::TilemapLength(bytes.len())),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layer2Storage {
    Objects,
    CompressedTilemap,
}

/// Mirrors Lunar Magic's `ClassifyLevelModeLayer2Storage` decision boundary.
#[must_use]
pub const fn level_mode_layer2_storage(level_mode: u8) -> Layer2Storage {
    match level_mode {
        0 | 9 | 10 | 11 | 12 | 13 | 14 | 16 | 17 | 18..=29 | 30 => Layer2Storage::CompressedTilemap,
        _ => Layer2Storage::Objects,
    }
}

#[derive(Debug)]
pub enum NativeLayer2Error {
    Objects(ObjectStreamError),
    TilemapLength(usize),
    CompressedTilemapLength(usize),
    TilemapCoordinate {
        x: usize,
        y: usize,
    },
    FloodPatternShape {
        width: usize,
        height: usize,
        words: usize,
    },
    TilemapRectangle {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    },
    TilemapMove {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        delta_x: i32,
        delta_y: i32,
    },
    LegacyHighByte {
        tile: usize,
        actual: u8,
        expected: u8,
    },
    LegacyUnusedWord {
        tile: usize,
    },
}

impl fmt::Display for NativeLayer2Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid native Layer 2 data: {self:?}")
    }
}

impl std::error::Error for NativeLayer2Error {}

impl From<ObjectStreamError> for NativeLayer2Error {
    fn from(value: ObjectStreamError) -> Self {
        Self::Objects(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_level_mode_classes_are_stable() {
        assert_eq!(
            level_mode_layer2_storage(0),
            Layer2Storage::CompressedTilemap
        );
        assert_eq!(level_mode_layer2_storage(1), Layer2Storage::Objects);
        assert_eq!(level_mode_layer2_storage(2), Layer2Storage::Objects);
        assert_eq!(
            level_mode_layer2_storage(0x1d),
            Layer2Storage::CompressedTilemap
        );
        assert_eq!(level_mode_layer2_storage(0x1f), Layer2Storage::Objects);
    }

    #[test]
    fn tilemap_canvas_index_is_a_complete_bijection() {
        assert_eq!(native_layer2_tilemap_index(0, 0), Some(0));
        assert_eq!(native_layer2_tilemap_index(1, 0), Some(16));
        assert_eq!(native_layer2_tilemap_index(31, 15), Some(511));
        assert_eq!(native_layer2_tilemap_index(0, 16), Some(512));
        assert_eq!(native_layer2_tilemap_index(31, 31), Some(1023));
        assert_eq!(native_layer2_tilemap_index(32, 0), None);
        assert_eq!(native_layer2_tilemap_index(0, 32), None);
        let mut indexes = (0..32)
            .flat_map(|y| (0..32).map(move |x| native_layer2_tilemap_index(x, y).unwrap()))
            .collect::<Vec<_>>();
        indexes.sort_unstable();
        assert_eq!(indexes, (0..1024).collect::<Vec<_>>());
    }

    #[test]
    fn flood_region_is_four_connected_exact_word_and_visual_ordered() {
        let mut words = vec![0_u16; 1024];
        for (x, y) in [(0, 0), (1, 0), (1, 1), (2, 1), (31, 0)] {
            words[native_layer2_tilemap_index(x, y).unwrap()] = 0x8123;
        }
        words[native_layer2_tilemap_index(2, 0).unwrap()] = 0x0123;
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            native_layer2_flood_region(&bytes, 0, 0).unwrap(),
            vec![
                native_layer2_tilemap_index(0, 0).unwrap(),
                native_layer2_tilemap_index(1, 0).unwrap(),
                native_layer2_tilemap_index(1, 1).unwrap(),
                native_layer2_tilemap_index(2, 1).unwrap(),
            ]
        );
        assert_eq!(
            native_layer2_flood_region(&bytes, 31, 0).unwrap(),
            vec![native_layer2_tilemap_index(31, 0).unwrap()]
        );
    }

    #[test]
    fn flood_region_rejects_wrong_shape_and_coordinates() {
        assert!(matches!(
            native_layer2_flood_region(&[0; NATIVE_LAYER2_TILEMAP_LEN - 1], 0, 0),
            Err(NativeLayer2Error::TilemapLength(_))
        ));
        assert!(matches!(
            native_layer2_flood_region(&[0; NATIVE_LAYER2_TILEMAP_LEN], 32, 0),
            Err(NativeLayer2Error::TilemapCoordinate { x: 32, y: 0 })
        ));
    }

    #[test]
    fn flood_pattern_repeats_from_region_minimum_bounds_and_masks_words() {
        let mut words = vec![0_u16; 1024];
        for (x, y) in [(5, 0), (3, 1), (4, 1), (5, 1), (3, 2)] {
            words[native_layer2_tilemap_index(x, y).unwrap()] = 0x7777;
        }
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        assert_eq!(
            native_layer2_flood_pattern(&bytes, 5, 0, 2, 2, &[0xf111, 0xf222, 0xf333, 0xf444],)
                .unwrap(),
            [
                ((5, 0), 0x0111),
                ((3, 1), 0x0333),
                ((4, 1), 0x0444),
                ((5, 1), 0x0333),
                ((3, 2), 0x0111),
            ]
            .map(|((x, y), word)| (native_layer2_tilemap_index(x, y).unwrap(), word))
        );
    }

    #[test]
    fn flood_pattern_rejects_empty_oversized_and_mismatched_shapes() {
        let bytes = [0; NATIVE_LAYER2_TILEMAP_LEN];
        for (width, height, words) in [
            (0, 1, Vec::new()),
            (1, 0, Vec::new()),
            (33, 1, vec![0; 33]),
            (1, 33, vec![0; 33]),
            (2, 2, vec![0; 3]),
        ] {
            assert!(matches!(
                native_layer2_flood_pattern(&bytes, 0, 0, width, height, &words),
                Err(NativeLayer2Error::FloodPatternShape { .. })
            ));
        }
    }

    #[test]
    fn rectangle_move_snapshots_before_clear_and_destination_wins_overlap() {
        let mut words = vec![0_u16; 1024];
        for (offset, word) in (1_u16..=6).enumerate() {
            let x = 1 + offset % 3;
            let y = 1 + offset / 3;
            words[native_layer2_tilemap_index(x, y).unwrap()] = word;
        }
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        let expected = [
            ((1, 1), 0),
            ((2, 1), 0),
            ((3, 1), 0),
            ((1, 2), 0),
            ((2, 2), 1),
            ((3, 2), 2),
            ((4, 2), 3),
            ((2, 3), 4),
            ((3, 3), 5),
            ((4, 3), 6),
        ]
        .map(|((x, y), word)| (native_layer2_tilemap_index(x, y).unwrap(), word));
        assert_eq!(
            native_layer2_move_rectangle(&bytes, 1, 1, 3, 2, 1, 1).unwrap(),
            expected
        );
        assert!(
            native_layer2_move_rectangle(&bytes, 1, 1, 3, 2, 0, 0)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rectangle_move_rejects_bad_source_and_every_crossed_edge() {
        let bytes = [0; NATIVE_LAYER2_TILEMAP_LEN];
        for (x, y, width, height) in [(0, 0, 0, 1), (0, 0, 1, 0), (31, 0, 2, 1), (0, 31, 1, 2)] {
            assert!(matches!(
                native_layer2_move_rectangle(&bytes, x, y, width, height, 0, 0),
                Err(NativeLayer2Error::TilemapRectangle { .. })
            ));
        }
        for (x, y, delta_x, delta_y) in
            [(0, 0, -1, 0), (0, 0, 0, -1), (31, 31, 1, 0), (31, 31, 0, 1)]
        {
            assert!(matches!(
                native_layer2_move_rectangle(&bytes, x, y, 1, 1, delta_x, delta_y),
                Err(NativeLayer2Error::TilemapMove { .. })
            ));
        }
    }

    #[test]
    fn flood_region_matches_every_three_by_three_binary_topology() {
        for mask in 0_u16..(1 << 9) {
            let mut words = vec![0x3333_u16; 1024];
            for y in 0..3 {
                for x in 0..3 {
                    let bit = 1 << (y * 3 + x);
                    words[native_layer2_tilemap_index(x, y).unwrap()] =
                        if mask & bit == 0 { 0x1111 } else { 0x2222 };
                }
            }
            let bytes = words
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect::<Vec<_>>();
            for start_y in 0..3 {
                for start_x in 0..3 {
                    let target_set = mask & (1 << (start_y * 3 + start_x)) != 0;
                    let mut expected = [false; 9];
                    let mut pending = vec![(start_x, start_y)];
                    while let Some((x, y)) = pending.pop() {
                        let visual = y * 3 + x;
                        if expected[visual] || (mask & (1 << visual) != 0) != target_set {
                            continue;
                        }
                        expected[visual] = true;
                        if x > 0 {
                            pending.push((x - 1, y));
                        }
                        if x < 2 {
                            pending.push((x + 1, y));
                        }
                        if y > 0 {
                            pending.push((x, y - 1));
                        }
                        if y < 2 {
                            pending.push((x, y + 1));
                        }
                    }
                    let expected = expected
                        .iter()
                        .enumerate()
                        .filter(|(_, included)| **included)
                        .map(|(visual, _)| {
                            native_layer2_tilemap_index(visual % 3, visual / 3).unwrap()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        native_layer2_flood_region(&bytes, start_x, start_y).unwrap(),
                        expected,
                        "mask {mask:#05x}, start ({start_x}, {start_y})"
                    );
                }
            }
        }
    }

    #[test]
    fn mwl_forms_round_trip() {
        let objects = NativeLayer2Data::decode_mwl(1, &[1, 2, 3, 4, 5, 0xff]).unwrap();
        assert_eq!(objects.encode_mwl().unwrap(), [1, 2, 3, 4, 5, 0xff]);
        let tilemap = vec![0x12; NATIVE_LAYER2_TILEMAP_LEN];
        assert_eq!(
            NativeLayer2Data::decode_mwl(0, &tilemap)
                .unwrap()
                .encode_mwl()
                .unwrap(),
            tilemap
        );
    }

    #[test]
    fn recovered_tilemap_layout_transforms_match_word_order() {
        let mut planes = vec![0; NATIVE_LAYER2_TILEMAP_LEN];
        planes[0] = 0x34;
        planes[0x400] = 0x12;
        let interleaved = interleave_layer2_tilemap_planes(&planes).unwrap();
        assert_eq!(&interleaved[..2], &[0x34, 0x12]);
        assert_eq!(split_layer2_tilemap_planes(&interleaved).unwrap(), planes);

        let legacy = vec![0xf1; LEGACY_LAYER2_TILEMAP_LEN];
        let expanded = expand_legacy_layer2_tilemap(&legacy, 0).unwrap();
        assert_eq!(&expanded[..4], &[0xf1, 0, 0xf1, 0]);
        assert_eq!(&expanded[0x360..0x400], &[0; 0xa0]);
        assert_eq!(&expanded[0x400..0x404], &[0xf1, 0, 0xf1, 0]);
        assert_eq!(compact_legacy_layer2_tilemap(&expanded, 0).unwrap(), legacy);
    }
}
