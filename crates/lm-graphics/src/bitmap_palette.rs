//! Lunar Magic-compatible color-option primitives for bitmap graphics imports.

use crate::{Bgr555, Palette, QuantizerError, Rgb8, Rgba8, WuQuantizer};
use std::{cmp::Ordering, collections::BTreeMap, fmt};

pub const BITMAP_PALETTE_ROWS: usize = 8;
pub const BITMAP_PALETTE_COLORS: usize = BITMAP_PALETTE_ROWS * Palette::COLORS_PER_ROW;

/// User-visible state of one entry in Lunar Magic's eight-row bitmap-import palette.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitmapPaletteEntryState {
    /// The importer may write a generated color here.
    Free,
    /// Preserve this color and make it available when assigning source tiles to rows.
    Reusable,
    /// Preserve this color but exclude it from imported artwork.
    Reserved,
}

impl BitmapPaletteEntryState {
    /// Returns the exact persistent state byte used by Lunar Magic's import workspace.
    #[must_use]
    pub const fn lunar_magic_bits(self) -> u8 {
        match self {
            Self::Free => 0,
            Self::Reusable => 4,
            Self::Reserved => 2,
        }
    }
}

/// High-color reduction choice exposed by Lunar Magic's color-options dialog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitmapPaletteReduction {
    MedianCut,
    Popularity,
}

/// Complete persistent color controls that precede per-tile palette-row assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BitmapPaletteColorOptions {
    pub entries: Vec<BitmapPaletteEntryState>,
    pub maximum_colors: usize,
    pub reduction: BitmapPaletteReduction,
    pub priority_level: u8,
}

impl BitmapPaletteColorOptions {
    /// Reconstructs the initialized Lunar Magic 3.63 option state proven at
    /// `InitializePaletteEntryUsageMap`.
    #[must_use]
    pub fn lunar_magic_initial() -> Self {
        let mut entries = vec![BitmapPaletteEntryState::Reserved; BITMAP_PALETTE_COLORS];
        for row in 0..BITMAP_PALETTE_ROWS {
            entries[row * Palette::COLORS_PER_ROW] = BitmapPaletteEntryState::Reusable;
        }
        for row in 0..2 {
            for entry in 1..=8 {
                entries[row * Palette::COLORS_PER_ROW + entry] = BitmapPaletteEntryState::Free;
            }
        }
        Self {
            entries,
            maximum_colors: BITMAP_PALETTE_COLORS,
            reduction: BitmapPaletteReduction::MedianCut,
            priority_level: 3,
        }
    }

    /// Validates the exact eight-row shape and recovered option bounds.
    ///
    /// # Errors
    ///
    /// Returns [`BitmapPaletteReductionError`] for a wrong entry count, a zero or over-128 color
    /// bound, or a priority outside the recovered inclusive 1–4 range.
    pub fn validate(&self) -> Result<(), BitmapPaletteReductionError> {
        if self.entries.len() != BITMAP_PALETTE_COLORS {
            return Err(BitmapPaletteReductionError::EntryCount(self.entries.len()));
        }
        if !(1..=BITMAP_PALETTE_COLORS).contains(&self.maximum_colors) {
            return Err(BitmapPaletteReductionError::MaximumColors(
                self.maximum_colors,
            ));
        }
        if !(1..=4).contains(&self.priority_level) {
            return Err(BitmapPaletteReductionError::PriorityLevel(
                self.priority_level,
            ));
        }
        Ok(())
    }
}

/// Globally reduced RGB555 colors and one color index per source pixel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReducedBitmapPalette {
    pub colors: Vec<Bgr555>,
    pub indices: Vec<u8>,
}

/// Complete eight-row palette assignment for one padded bitmap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiRowBitmapPalette {
    pub palette: Palette,
    /// One local 0–15 palette index per source pixel.
    pub indices: Vec<u8>,
    /// One 0–7 palette row per source 8×8 tile, in row-major order.
    pub tile_rows: Vec<u8>,
    pub generated_colors: usize,
}

/// Applies the selected global 1–128-color reduction before palette-row assignment.
///
/// Transparent pixels receive index zero in `indices` and do not consume a reduced color. Opaque
/// indexes are stored one-based so zero remains unambiguous. When the source already fits the
/// bound, colors are ordered by RGB555 value. Popularity uses the recovered frequency admission
/// gate and optional destination-aware distance-priority score; median-cut delegates to the
/// deterministic variance-splitting quantizer.
///
/// # Errors
///
/// Returns [`BitmapPaletteReductionError`] for invalid options, fractional alpha, excessive
/// quantizer input, or an unrepresentable one-based color index.
pub fn reduce_bitmap_palette(
    pixels: &[Rgba8],
    options: &BitmapPaletteColorOptions,
) -> Result<ReducedBitmapPalette, BitmapPaletteReductionError> {
    reduce_bitmap_palette_internal(pixels, None, options)
}

/// Applies bitmap reduction with the destination palette available to the Popularity priority
/// scorer recovered from Lunar Magic.
///
/// Reusable entries influence high-color selection: colors farther from both those preserved
/// entries and already selected colors receive the recovered distance-exponent bonus. Median Cut
/// does not consume destination-palette context.
///
/// # Errors
///
/// Returns the same validation, alpha, quantizer, and index errors as
/// [`reduce_bitmap_palette`], plus a palette-shape error when Popularity receives fewer than 128
/// destination colors.
pub fn reduce_bitmap_palette_with_palette(
    pixels: &[Rgba8],
    original: &Palette,
    options: &BitmapPaletteColorOptions,
) -> Result<ReducedBitmapPalette, BitmapPaletteReductionError> {
    reduce_bitmap_palette_internal(pixels, Some(original), options)
}

fn reduce_bitmap_palette_internal(
    pixels: &[Rgba8],
    original: Option<&Palette>,
    options: &BitmapPaletteColorOptions,
) -> Result<ReducedBitmapPalette, BitmapPaletteReductionError> {
    options.validate()?;
    let mut opaque = Vec::with_capacity(pixels.len());
    for (index, pixel) in pixels.iter().enumerate() {
        match pixel.alpha {
            0 => {}
            255 => opaque.push(Rgb8 {
                red: pixel.red,
                green: pixel.green,
                blue: pixel.blue,
            }),
            alpha => {
                return Err(BitmapPaletteReductionError::FractionalAlpha { index, alpha });
            }
        }
    }
    if opaque.is_empty() {
        return Ok(ReducedBitmapPalette {
            colors: Vec::new(),
            indices: vec![0; pixels.len()],
        });
    }
    let mut histogram = BTreeMap::<u16, usize>::new();
    for pixel in &opaque {
        *histogram.entry(Bgr555::from_rgb8(*pixel).0).or_default() += 1;
    }
    let colors = if histogram.len() <= options.maximum_colors {
        histogram.keys().copied().map(Bgr555).collect()
    } else {
        match options.reduction {
            BitmapPaletteReduction::MedianCut => {
                WuQuantizer::quantize(&opaque, options.maximum_colors)
                    .map_err(BitmapPaletteReductionError::Quantizer)?
                    .palette
                    .colors
            }
            BitmapPaletteReduction::Popularity => {
                select_popularity_colors(&histogram, original, options)?
            }
        }
    };
    let palette = Palette {
        colors: colors.clone(),
    };
    let mut opaque_indices = palette
        .quantize(&opaque)
        .ok_or(BitmapPaletteReductionError::EmptyOpaquePalette)?
        .into_iter();
    let indices = pixels
        .iter()
        .map(|pixel| {
            if pixel.alpha == 0 {
                Ok(0)
            } else {
                opaque_indices
                    .next()
                    .ok_or(BitmapPaletteReductionError::IndexPlaneMismatch)?
                    .checked_add(1)
                    .and_then(|index| u8::try_from(index).ok())
                    .ok_or(BitmapPaletteReductionError::IndexOverflow)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if opaque_indices.next().is_some() {
        return Err(BitmapPaletteReductionError::IndexPlaneMismatch);
    }
    Ok(ReducedBitmapPalette { colors, indices })
}

fn select_popularity_colors(
    histogram: &BTreeMap<u16, usize>,
    original: Option<&Palette>,
    options: &BitmapPaletteColorOptions,
) -> Result<Vec<Bgr555>, BitmapPaletteReductionError> {
    let reusable = if let Some(palette) = original {
        if palette.colors.len() < BITMAP_PALETTE_COLORS {
            return Err(BitmapPaletteReductionError::PaletteColors(
                palette.colors.len(),
            ));
        }
        options
            .entries
            .iter()
            .zip(&palette.colors)
            .filter_map(|(state, color)| {
                (*state == BitmapPaletteEntryState::Reusable).then_some(color.0)
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut selected = Vec::<(u16, u32)>::with_capacity(options.maximum_colors);
    for (&color, &frequency) in histogram {
        let frequency = u32::try_from(frequency).unwrap_or(u32::MAX);
        if selected.len() == options.maximum_colors
            && selected
                .last()
                .is_some_and(|(_, minimum_score)| *minimum_score >= frequency)
        {
            continue;
        }
        let mut score = frequency;
        let nearest = reusable
            .iter()
            .chain(selected.iter().map(|(selected, _)| selected))
            .map(|candidate| lunar_magic_color_distance(color, *candidate))
            .min();
        if let Some(mut distance) = nearest {
            for _ in 1..options.priority_level {
                distance = distance.wrapping_mul(distance);
            }
            score = score.wrapping_add(distance.wrapping_mul(frequency).wrapping_div(0x8e_e09));
        }
        let insertion = selected.partition_point(|(selected_color, selected_score)| {
            (*selected_score, std::cmp::Reverse(*selected_color))
                >= (score, std::cmp::Reverse(color))
        });
        if insertion < options.maximum_colors {
            selected.insert(insertion, (color, score));
            selected.truncate(options.maximum_colors);
        }
    }
    Ok(selected
        .into_iter()
        .map(|(color, _)| Bgr555(color))
        .collect())
}

/// Assigns globally reduced source colors to Lunar Magic's eight 16-color rows.
///
/// Source dimensions must be complete 8×8 tiles. Unique tile color sets are weighted by their
/// occurrence count plus the count of all subset sets. The allocator repeatedly selects the
/// highest-weight unassigned set, chooses the capable row with greatest existing-color overlap
/// (then least remaining free capacity and lowest row), installs missing colors in ascending free
/// entry order, and marks every still-unassigned subset that the resulting row covers. The final
/// pass independently scores every 8×8 tile against each usable row with Lunar Magic's weighted
/// RGB555 distance, selects the least-error row, and converts its pixels to that row's nearest
/// entries. A source color therefore need not have been installed exactly.
///
/// Reusable colors retain their exact palette indexes. Reserved entries are neither overwritten
/// nor candidates. Entry zero remains transparency-only, so an opaque tile requiring more than 15
/// distinct reduced colors is rejected.
///
/// # Errors
///
/// Returns [`BitmapPaletteReductionError`] for malformed geometry/planes/palette shape, invalid
/// options, an excessive tile color set, or an opaque tile with no usable palette row.
pub fn allocate_bitmap_palette_rows(
    reduced: &ReducedBitmapPalette,
    width: usize,
    height: usize,
    original: &Palette,
    options: &BitmapPaletteColorOptions,
) -> Result<MultiRowBitmapPalette, BitmapPaletteReductionError> {
    options.validate()?;
    if width == 0 || height == 0 || width % 8 != 0 || height % 8 != 0 {
        return Err(BitmapPaletteReductionError::TileGeometry { width, height });
    }
    let pixel_count = width
        .checked_mul(height)
        .ok_or(BitmapPaletteReductionError::PixelCount {
            expected: usize::MAX,
            actual: reduced.indices.len(),
        })?;
    if reduced.indices.len() != pixel_count {
        return Err(BitmapPaletteReductionError::PixelCount {
            expected: pixel_count,
            actual: reduced.indices.len(),
        });
    }
    if original.colors.len() < BITMAP_PALETTE_COLORS {
        return Err(BitmapPaletteReductionError::PaletteColors(
            original.colors.len(),
        ));
    }
    let tiles_wide = width / 8;
    let tiles_high = height / 8;
    let tile_sets = build_tile_color_sets(reduced, width, tiles_wide, tiles_high)?;
    let mut records = build_color_set_records(&tile_sets);
    let mut rows: [PaletteRowAllocation; BITMAP_PALETTE_ROWS] =
        std::array::from_fn(|row| PaletteRowAllocation::new(row, original, options));
    assign_color_set_records(&mut records, &mut rows)?;

    let mut palette = original.clone();
    let mut generated_colors = 0;
    for row in &rows {
        for (entry, color) in row.entries.iter().enumerate() {
            if let RowEntry::Assigned(color) = color {
                palette.colors[row.row * Palette::COLORS_PER_ROW + entry] = Bgr555(*color);
                generated_colors += 1;
            }
        }
    }
    let (indices, tile_rows) =
        assign_tiles_to_lowest_error_rows(reduced, width, tiles_wide, tiles_high, &rows)?;
    Ok(MultiRowBitmapPalette {
        palette,
        indices,
        tile_rows,
        generated_colors,
    })
}

fn build_tile_color_sets(
    reduced: &ReducedBitmapPalette,
    width: usize,
    tiles_wide: usize,
    tiles_high: usize,
) -> Result<Vec<TileColorHistogram>, BitmapPaletteReductionError> {
    let mut tile_sets = Vec::with_capacity(tiles_wide * tiles_high);
    for tile_y in 0..tiles_high {
        for tile_x in 0..tiles_wide {
            let mut histogram = BTreeMap::<u16, usize>::new();
            for pixel_y in 0..8 {
                let row = (tile_y * 8 + pixel_y) * width + tile_x * 8;
                for index in &reduced.indices[row..row + 8] {
                    if *index != 0 {
                        let color = reduced
                            .colors
                            .get(usize::from(*index) - 1)
                            .ok_or(BitmapPaletteReductionError::ReducedIndex(*index))?;
                        *histogram.entry(color.0).or_default() += 1;
                    }
                }
            }
            if histogram.len() > 15 {
                return Err(BitmapPaletteReductionError::TileColors {
                    tile: tile_sets.len(),
                    colors: histogram.len(),
                });
            }
            let (colors, weights) = histogram.into_iter().unzip();
            tile_sets.push(TileColorHistogram { colors, weights });
        }
    }
    Ok(tile_sets)
}

fn build_color_set_records(tile_sets: &[TileColorHistogram]) -> BTreeMap<Vec<u16>, ColorSetRecord> {
    let mut records = BTreeMap::<Vec<u16>, ColorSetRecord>::new();
    for (tile, histogram) in tile_sets.iter().enumerate() {
        let record = records
            .entry(histogram.colors.clone())
            .or_insert_with(|| ColorSetRecord {
                colors: histogram.colors.clone(),
                tiles: Vec::new(),
                direct_weights: vec![0; histogram.colors.len()],
                aggregate_weights: Vec::new(),
                aggregate_weight: 0,
                assigned_row: None,
            });
        record.tiles.push(tile);
        for (weight, contribution) in record.direct_weights.iter_mut().zip(&histogram.weights) {
            *weight += contribution;
        }
    }
    let keys = records.keys().cloned().collect::<Vec<_>>();
    let weights = keys
        .iter()
        .map(|key| {
            let mut aggregate = records[key].direct_weights.clone();
            for subset in keys
                .iter()
                .filter(|subset| subset.len() < key.len() && is_subset(subset, key))
            {
                for (color, weight) in subset.iter().zip(&records[subset].direct_weights) {
                    let destination = key
                        .binary_search(color)
                        .expect("a proven subset color is present");
                    aggregate[destination] += weight;
                }
            }
            aggregate
        })
        .collect::<Vec<_>>();
    for (key, weights) in keys.iter().zip(weights) {
        if let Some(record) = records.get_mut(key) {
            record.aggregate_weight = weights.iter().sum();
            record.aggregate_weights = weights;
        }
    }
    records
}

fn assign_color_set_records(
    records: &mut BTreeMap<Vec<u16>, ColorSetRecord>,
    rows: &mut [PaletteRowAllocation; BITMAP_PALETTE_ROWS],
) -> Result<(), BitmapPaletteReductionError> {
    loop {
        let next = records
            .values()
            .filter(|record| {
                record.assigned_row.is_none() && best_palette_row(rows, &record.colors).is_some()
            })
            .max_by(|left, right| compare_color_set_priority(left, right))
            .map(|record| record.colors.clone());
        let Some(colors) = next else {
            return Ok(());
        };
        if colors.is_empty() {
            let record = records
                .get_mut(&colors)
                .ok_or_else(|| BitmapPaletteReductionError::UnassignedColorSet(colors.clone()))?;
            record.assigned_row = Some(0);
            continue;
        }
        let row = best_palette_row(rows, &colors)
            .ok_or_else(|| BitmapPaletteReductionError::UnassignedColorSet(colors.clone()))?;
        rows[row].install(&colors)?;
        let covered = rows[row].colors();
        for record in records.values_mut() {
            if record.assigned_row.is_none() && is_subset(&record.colors, &covered) {
                record.assigned_row = Some(row);
            }
        }
    }
}

fn assign_tiles_to_lowest_error_rows(
    reduced: &ReducedBitmapPalette,
    width: usize,
    tiles_wide: usize,
    tiles_high: usize,
    rows: &[PaletteRowAllocation; BITMAP_PALETTE_ROWS],
) -> Result<(Vec<u8>, Vec<u8>), BitmapPaletteReductionError> {
    let mut indices = vec![0; reduced.indices.len()];
    let mut tile_rows = Vec::with_capacity(tiles_wide * tiles_high);
    for tile_y in 0..tiles_high {
        for tile_x in 0..tiles_wide {
            let mut best: Option<(u64, usize, [u8; 64])> = None;
            for row in rows {
                let Some((error, tile_indices)) =
                    score_tile_for_row(reduced, width, tile_x, tile_y, row)?
                else {
                    continue;
                };
                if best.as_ref().is_none_or(|(best_error, best_row, _)| {
                    (error, row.row) < (*best_error, *best_row)
                }) {
                    best = Some((error, row.row, tile_indices));
                }
            }
            let Some((_, row, tile_indices)) = best else {
                return Err(BitmapPaletteReductionError::NoEligiblePaletteRow {
                    tile: tile_rows.len(),
                });
            };
            tile_rows.push(
                u8::try_from(row).map_err(|_| BitmapPaletteReductionError::RowOverflow(row))?,
            );
            for pixel_y in 0..8 {
                let destination = (tile_y * 8 + pixel_y) * width + tile_x * 8;
                let source = pixel_y * 8;
                indices[destination..destination + 8]
                    .copy_from_slice(&tile_indices[source..source + 8]);
            }
        }
    }
    Ok((indices, tile_rows))
}

fn score_tile_for_row(
    reduced: &ReducedBitmapPalette,
    width: usize,
    tile_x: usize,
    tile_y: usize,
    row: &PaletteRowAllocation,
) -> Result<Option<(u64, [u8; 64])>, BitmapPaletteReductionError> {
    let mut error = 0_u64;
    let mut indices = [0; 64];
    for pixel_y in 0..8 {
        let source_offset = (tile_y * 8 + pixel_y) * width + tile_x * 8;
        for pixel_x in 0..8 {
            let source_index = reduced.indices[source_offset + pixel_x];
            if source_index == 0 {
                continue;
            }
            let color = reduced
                .colors
                .get(usize::from(source_index) - 1)
                .ok_or(BitmapPaletteReductionError::ReducedIndex(source_index))?
                .0;
            let Some((entry, distance)) = row.nearest(color) else {
                return Ok(None);
            };
            indices[pixel_y * 8 + pixel_x] = entry;
            error = error.saturating_add(u64::from(distance));
        }
    }
    Ok(Some((error, indices)))
}

fn lunar_magic_color_distance(left: u16, right: u16) -> u32 {
    let red = i32::from(left & 0x1f) - i32::from(right & 0x1f);
    let green = i32::from((left >> 5) & 0x1f) - i32::from((right >> 5) & 0x1f);
    let blue = i32::from((left >> 10) & 0x1f) - i32::from((right >> 10) & 0x1f);
    u32::try_from(red * red * 4 + green * green * 3 + blue * blue * 2).unwrap_or(u32::MAX)
}

fn best_palette_row(
    rows: &[PaletteRowAllocation; BITMAP_PALETTE_ROWS],
    colors: &[u16],
) -> Option<usize> {
    rows.iter()
        .filter_map(|row| row.score(colors).map(|score| (row.row, score)))
        .max_by(|(left_row, left), (right_row, right)| {
            left.overlap
                .cmp(&right.overlap)
                .then_with(|| right.free_before.cmp(&left.free_before))
                .then_with(|| right_row.cmp(left_row))
        })
        .map(|(row, _)| row)
}

#[derive(Clone, Debug)]
struct TileColorHistogram {
    colors: Vec<u16>,
    weights: Vec<usize>,
}

#[derive(Clone, Debug)]
struct ColorSetRecord {
    colors: Vec<u16>,
    tiles: Vec<usize>,
    direct_weights: Vec<usize>,
    aggregate_weights: Vec<usize>,
    aggregate_weight: usize,
    assigned_row: Option<usize>,
}

fn compare_color_set_priority(left: &ColorSetRecord, right: &ColorSetRecord) -> Ordering {
    left.aggregate_weight
        .cmp(&right.aggregate_weight)
        .then_with(|| left.aggregate_weights.cmp(&right.aggregate_weights))
        .then_with(|| left.colors.len().cmp(&right.colors.len()))
        .then_with(|| right.colors.cmp(&left.colors))
}

fn is_subset(subset: &[u16], superset: &[u16]) -> bool {
    subset
        .iter()
        .all(|color| superset.binary_search(color).is_ok())
}

#[derive(Clone, Copy, Debug)]
enum RowEntry {
    Reserved,
    Free,
    Reusable(u16),
    Assigned(u16),
}

#[derive(Debug)]
struct PaletteRowAllocation {
    row: usize,
    entries: [RowEntry; Palette::COLORS_PER_ROW],
}

#[derive(Clone, Copy)]
struct RowScore {
    overlap: usize,
    free_before: usize,
}

impl PaletteRowAllocation {
    fn new(row: usize, original: &Palette, options: &BitmapPaletteColorOptions) -> Self {
        let entries = std::array::from_fn(|entry| {
            let index = row * Palette::COLORS_PER_ROW + entry;
            match options.entries[index] {
                BitmapPaletteEntryState::Free if entry != 0 => RowEntry::Free,
                BitmapPaletteEntryState::Reusable if entry != 0 => {
                    RowEntry::Reusable(original.colors[index].0)
                }
                BitmapPaletteEntryState::Free
                | BitmapPaletteEntryState::Reusable
                | BitmapPaletteEntryState::Reserved => RowEntry::Reserved,
            }
        });
        Self { row, entries }
    }

    fn score(&self, colors: &[u16]) -> Option<RowScore> {
        let overlap = colors
            .iter()
            .filter(|color| self.index_of(**color).is_some())
            .count();
        let free_before = self
            .entries
            .iter()
            .filter(|entry| matches!(entry, RowEntry::Free))
            .count();
        (colors.len() - overlap <= free_before).then_some(RowScore {
            overlap,
            free_before,
        })
    }

    fn install(&mut self, colors: &[u16]) -> Result<(), BitmapPaletteReductionError> {
        for color in colors {
            if self.index_of(*color).is_some() {
                continue;
            }
            let entry = self
                .entries
                .iter_mut()
                .find(|entry| matches!(entry, RowEntry::Free))
                .ok_or(BitmapPaletteReductionError::RowCapacity(self.row))?;
            *entry = RowEntry::Assigned(*color);
        }
        Ok(())
    }

    fn index_of(&self, color: u16) -> Option<u8> {
        self.entries
            .iter()
            .position(|entry| {
                matches!(entry, RowEntry::Reusable(value) | RowEntry::Assigned(value) if *value == color)
            })
            .and_then(|entry| u8::try_from(entry).ok())
    }

    fn nearest(&self, color: u16) -> Option<(u8, u32)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(entry, candidate)| match candidate {
                RowEntry::Reusable(value) | RowEntry::Assigned(value) => {
                    let index = u8::try_from(entry).ok()?;
                    Some((index, lunar_magic_color_distance(color, *value)))
                }
                RowEntry::Reserved | RowEntry::Free => None,
            })
            .min_by_key(|(entry, distance)| (*distance, *entry))
    }

    fn colors(&self) -> Vec<u16> {
        let mut colors = self
            .entries
            .iter()
            .filter_map(|entry| match entry {
                RowEntry::Reusable(color) | RowEntry::Assigned(color) => Some(*color),
                RowEntry::Reserved | RowEntry::Free => None,
            })
            .collect::<Vec<_>>();
        colors.sort_unstable();
        colors.dedup();
        colors
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BitmapPaletteReductionError {
    EntryCount(usize),
    MaximumColors(usize),
    PriorityLevel(u8),
    FractionalAlpha { index: usize, alpha: u8 },
    Quantizer(QuantizerError),
    EmptyOpaquePalette,
    IndexPlaneMismatch,
    IndexOverflow,
    TileGeometry { width: usize, height: usize },
    PixelCount { expected: usize, actual: usize },
    PaletteColors(usize),
    ReducedIndex(u8),
    TileColors { tile: usize, colors: usize },
    UnassignedColorSet(Vec<u16>),
    RowOverflow(usize),
    RowCapacity(usize),
    RowColorMissing { row: usize, color: u16 },
    NoEligiblePaletteRow { tile: usize },
}

impl fmt::Display for BitmapPaletteReductionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bitmap palette reduction failed: {self:?}")
    }
}

impl std::error::Error for BitmapPaletteReductionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(pixel: Rgba8) -> Rgb8 {
        Rgb8 {
            red: pixel.red,
            green: pixel.green,
            blue: pixel.blue,
        }
    }

    #[test]
    fn recovered_initial_state_has_exact_rows_bits_and_bounds() {
        let options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.validate().unwrap();
        assert_eq!(options.maximum_colors, 128);
        assert_eq!(options.priority_level, 3);
        for row in 0..8 {
            let start = row * 16;
            assert_eq!(options.entries[start].lunar_magic_bits(), 4);
            for entry in 1..16 {
                let expected = if row < 2 && entry <= 8 { 0 } else { 2 };
                assert_eq!(options.entries[start + entry].lunar_magic_bits(), expected);
            }
        }
    }

    #[test]
    fn popularity_keeps_the_high_frequency_gate_before_distance_priority() {
        let mut options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.maximum_colors = 2;
        options.reduction = BitmapPaletteReduction::Popularity;
        let red = Rgba8 {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        let green = Rgba8 {
            red: 0,
            green: 255,
            blue: 0,
            alpha: 255,
        };
        let blue = Rgba8 {
            red: 0,
            green: 0,
            blue: 255,
            alpha: 255,
        };
        let reduced = reduce_bitmap_palette(&[red, red, green, blue], &options).unwrap();
        assert!(reduced.colors.contains(&Bgr555::from_rgb8(rgb(red))));
        assert!(reduced.colors.contains(&Bgr555::from_rgb8(rgb(green))));
        assert!(!reduced.colors.contains(&Bgr555::from_rgb8(rgb(blue))));
        assert!(reduced.indices.iter().all(|index| (1..=2).contains(index)));
    }

    #[test]
    fn popularity_priority_uses_reusable_destination_colors() {
        let histogram = BTreeMap::from([(1, 100), (0x03e0, 95), (0x7fff, 96)]);
        let palette = Palette {
            colors: vec![Bgr555(0); BITMAP_PALETTE_COLORS],
        };
        let mut options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.entries.fill(BitmapPaletteEntryState::Reserved);
        options.entries[0] = BitmapPaletteEntryState::Reusable;
        options.maximum_colors = 2;
        options.priority_level = 1;
        let low = select_popularity_colors(&histogram, Some(&palette), &options).unwrap();
        options.priority_level = 4;
        let high = select_popularity_colors(&histogram, Some(&palette), &options).unwrap();

        assert_eq!(low, vec![Bgr555(1), Bgr555(0x7fff)]);
        assert_eq!(high, vec![Bgr555(0x03e0), Bgr555(1)]);
    }

    #[test]
    fn transparency_is_zero_and_fractional_alpha_is_rejected() {
        let options = BitmapPaletteColorOptions::lunar_magic_initial();
        let transparent = Rgba8 {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        };
        let opaque = Rgba8 {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        };
        let reduced = reduce_bitmap_palette(&[transparent, opaque], &options).unwrap();
        assert_eq!(reduced.indices[0], 0);
        assert!(reduced.indices[1] > 0);
        assert!(matches!(
            reduce_bitmap_palette(
                &[Rgba8 {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 1,
                }],
                &options
            ),
            Err(BitmapPaletteReductionError::FractionalAlpha { .. })
        ));
    }

    fn palette() -> Palette {
        Palette {
            colors: vec![Bgr555(0); BITMAP_PALETTE_COLORS],
        }
    }

    fn reserved_options() -> BitmapPaletteColorOptions {
        BitmapPaletteColorOptions {
            entries: vec![BitmapPaletteEntryState::Reserved; BITMAP_PALETTE_COLORS],
            maximum_colors: BITMAP_PALETTE_COLORS,
            reduction: BitmapPaletteReduction::MedianCut,
            priority_level: 3,
        }
    }

    #[test]
    fn allocator_prefers_reusable_overlap_and_preserves_its_exact_index() {
        let red = Bgr555(0x001f);
        let blue = Bgr555(0x7c00);
        let mut original = palette();
        original.colors[1] = red;
        let mut options = reserved_options();
        options.entries[1] = BitmapPaletteEntryState::Reusable;
        options.entries[2] = BitmapPaletteEntryState::Free;
        options.entries[17] = BitmapPaletteEntryState::Free;
        options.entries[18] = BitmapPaletteEntryState::Free;
        let reduced = ReducedBitmapPalette {
            colors: vec![red, blue],
            indices: (0..64)
                .map(|index| if index & 1 == 0 { 1 } else { 2 })
                .collect(),
        };
        let allocated = allocate_bitmap_palette_rows(&reduced, 8, 8, &original, &options).unwrap();
        assert_eq!(allocated.tile_rows, [0]);
        assert_eq!(allocated.palette.colors[1], red);
        assert_eq!(allocated.palette.colors[2], blue);
        assert_eq!(allocated.generated_colors, 1);
        assert_eq!(&allocated.indices[..4], &[1, 2, 1, 2]);
    }

    #[test]
    fn disjoint_full_sets_are_distributed_across_capable_rows() {
        let mut options = reserved_options();
        for index in [1, 2, 17, 18] {
            options.entries[index] = BitmapPaletteEntryState::Free;
        }
        let reduced = ReducedBitmapPalette {
            colors: vec![Bgr555(1), Bgr555(2), Bgr555(3), Bgr555(4)],
            indices: (0..8)
                .flat_map(|_| {
                    (0..16).map(|x| match x {
                        0..=3 => 1,
                        4..=7 => 2,
                        8..=11 => 3,
                        _ => 4,
                    })
                })
                .collect(),
        };
        let allocated =
            allocate_bitmap_palette_rows(&reduced, 16, 8, &palette(), &options).unwrap();
        assert_ne!(allocated.tile_rows[0], allocated.tile_rows[1]);
        assert_eq!(allocated.generated_colors, 4);
    }

    #[test]
    fn subset_tiles_share_the_superset_row() {
        let mut options = reserved_options();
        options.entries[1] = BitmapPaletteEntryState::Free;
        options.entries[2] = BitmapPaletteEntryState::Free;
        let reduced = ReducedBitmapPalette {
            colors: vec![Bgr555(1), Bgr555(2)],
            indices: (0..8)
                .flat_map(|_| (0..16).map(|x| if x >= 8 || x & 1 == 0 { 1 } else { 2 }))
                .collect(),
        };
        let allocated =
            allocate_bitmap_palette_rows(&reduced, 16, 8, &palette(), &options).unwrap();
        assert_eq!(allocated.tile_rows[0], allocated.tile_rows[1]);
        assert_eq!(allocated.generated_colors, 2);
    }

    #[test]
    fn final_tile_assignment_uses_nearest_row_colors_when_exact_set_cannot_fit() {
        let red = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        let green = Bgr555::from_rgb8(Rgb8 {
            red: 0,
            green: 255,
            blue: 0,
        });
        let blue = Bgr555::from_rgb8(Rgb8 {
            red: 0,
            green: 0,
            blue: 255,
        });
        let reduced = ReducedBitmapPalette {
            colors: vec![red, green, blue],
            indices: (0..64)
                .map(|pixel| u8::try_from(pixel % 3 + 1).unwrap())
                .collect(),
        };
        let mut palette = Palette {
            colors: vec![Bgr555(0); BITMAP_PALETTE_COLORS],
        };
        palette.colors[1] = red;
        palette.colors[2] = green;
        let mut options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.entries.fill(BitmapPaletteEntryState::Reserved);
        options.entries[1] = BitmapPaletteEntryState::Reusable;
        options.entries[2] = BitmapPaletteEntryState::Reusable;

        let allocated = allocate_bitmap_palette_rows(&reduced, 8, 8, &palette, &options).unwrap();

        assert_eq!(allocated.generated_colors, 0);
        assert_eq!(allocated.tile_rows, vec![0]);
        assert_eq!(allocated.indices[0], 1);
        assert_eq!(allocated.indices[1], 2);
        assert!(matches!(allocated.indices[2], 1 | 2));
        assert_eq!(allocated.palette, palette);
    }

    #[test]
    fn color_set_aggregation_retains_pixel_frequency_from_strict_subsets() {
        let records = build_color_set_records(&[
            TileColorHistogram {
                colors: vec![1, 2],
                weights: vec![63, 1],
            },
            TileColorHistogram {
                colors: vec![1],
                weights: vec![64],
            },
            TileColorHistogram {
                colors: vec![2],
                weights: vec![64],
            },
        ]);
        let superset = &records[&vec![1, 2]];
        assert_eq!(superset.direct_weights, vec![63, 1]);
        assert_eq!(superset.aggregate_weights, vec![127, 65]);
        assert_eq!(superset.aggregate_weight, 192);
    }

    #[test]
    fn opaque_tile_with_sixteen_colors_is_rejected() {
        let colors = (0_u16..16).map(Bgr555).collect::<Vec<_>>();
        let reduced = ReducedBitmapPalette {
            colors,
            indices: (0..64)
                .map(|index| u8::try_from(index % 16 + 1).unwrap())
                .collect(),
        };
        assert!(matches!(
            allocate_bitmap_palette_rows(
                &reduced,
                8,
                8,
                &palette(),
                &BitmapPaletteColorOptions::lunar_magic_initial()
            ),
            Err(BitmapPaletteReductionError::TileColors {
                tile: 0,
                colors: 16
            })
        ));
    }
}
