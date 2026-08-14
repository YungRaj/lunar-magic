//! Lunar Magic-compatible color-option primitives for bitmap graphics imports.

use crate::{Bgr555, Palette, QuantizerError, Rgb8, Rgba8, WuQuantizer};
use std::{
    cmp::{Ordering, Reverse},
    collections::BTreeMap,
    fmt,
};

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
    /// Gives colors farther from reusable and already-selected colors extra admission weight.
    pub prioritize_unique_colors: bool,
    /// Keeps the exact-fit allocation pass and skips the weighted partial-set extension pass.
    pub maintain_detail: bool,
    /// Allows colors in free entries to be replaced by colors generated from the bitmap.
    pub allow_modifying_unmarked_colors: bool,
    /// Persistent native preference whose 3.63 control has no processing-path reader.
    pub prioritize_exact_palette_matches: bool,
    /// Maximum circular HSL240 hue distance when substituting a preserved palette color.
    pub reusable_color_hue_tolerance: u16,
    /// Lunar Magic's first high-color neighborhood reduction pass.
    pub popularity_reduction_method_1: bool,
    /// Lunar Magic's second high-color neighborhood reduction pass.
    pub popularity_reduction_method_2: bool,
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
            prioritize_unique_colors: true,
            maintain_detail: false,
            allow_modifying_unmarked_colors: true,
            prioritize_exact_palette_matches: true,
            reusable_color_hue_tolerance: 45,
            popularity_reduction_method_1: true,
            popularity_reduction_method_2: false,
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
        if self.reusable_color_hue_tolerance > 240 {
            return Err(BitmapPaletteReductionError::HueTolerance(
                self.reusable_color_hue_tolerance,
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
    if !options.allow_modifying_unmarked_colors && original.is_none() {
        return Err(BitmapPaletteReductionError::OriginalPaletteRequired);
    }
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
        *histogram
            .entry(lunar_magic_bitmap_color(*pixel).0)
            .or_default() += 1;
    }
    let mut colors = if options.allow_modifying_unmarked_colors {
        reduce_installable_colors(&opaque, &histogram, original, options)?
    } else {
        collect_unique_available_palette_colors(
            original.expect("the no-modification path checked its palette above"),
            options,
        )?
    };
    let mut exact_existing = Vec::new();
    if options.prioritize_exact_palette_matches
        && let Some(original) = original
    {
        // Opaque black already present in a usable, nonzero destination entry bypasses the
        // generated-color limit. Lunar Magic retains that cell before reducing the remaining
        // colors; this is observable with a one-color limit and separate black/red blocks.
        for color in histogram.keys().copied() {
            let available = options
                .entries
                .iter()
                .zip(&original.colors)
                .enumerate()
                .any(|(index, (state, candidate))| {
                    index % Palette::COLORS_PER_ROW != 0
                        && *state != BitmapPaletteEntryState::Reserved
                        && candidate.0 == color
                });
            let already_reduced = colors.iter().any(|candidate| candidate.0 == color);
            if available && color == 0 && !already_reduced {
                colors.push(Bgr555(color));
            }
            if available && color == 0 {
                exact_existing.push(color);
            }
        }
    }
    let opaque_indices = if options.maintain_detail {
        let mut indices = maintain_detail_palette_indices(&opaque, &colors, &exact_existing)?;
        // Maintain Detail includes a temporary zero-color candidate before the reduced colors.
        // Its index zero is not bitmap transparency: opaque pixels assigned to it continue into
        // row allocation and may use an existing nonzero palette cell containing black. Keep that
        // candidate in the reduced palette so only source alpha produces an actual zero index.
        if indices.contains(&0) {
            let black = colors
                .iter()
                .position(|color| color.0 == 0)
                .unwrap_or_else(|| {
                    colors.push(Bgr555(0));
                    colors.len() - 1
                });
            let black =
                u8::try_from(black + 1).map_err(|_| BitmapPaletteReductionError::IndexOverflow)?;
            for index in &mut indices {
                if *index == 0 {
                    *index = black;
                }
            }
        }
        indices
    } else {
        nearest_lunar_magic_palette_indices(&opaque, &colors)?
    };
    let mut opaque_indices = opaque_indices.into_iter();
    let indices = pixels
        .iter()
        .map(|pixel| {
            if pixel.alpha == 0 {
                Ok(0)
            } else {
                opaque_indices
                    .next()
                    .ok_or(BitmapPaletteReductionError::IndexPlaneMismatch)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if opaque_indices.next().is_some() {
        return Err(BitmapPaletteReductionError::IndexPlaneMismatch);
    }
    Ok(ReducedBitmapPalette { colors, indices })
}

fn reduce_installable_colors(
    opaque: &[Rgb8],
    histogram: &BTreeMap<u16, usize>,
    original: Option<&Palette>,
    options: &BitmapPaletteColorOptions,
) -> Result<Vec<Bgr555>, BitmapPaletteReductionError> {
    let mut requested = native_reduction_color_limit(original, options);
    loop {
        let mut colors = if histogram.len() <= requested {
            histogram.keys().copied().map(Bgr555).collect()
        } else {
            match options.reduction {
                BitmapPaletteReduction::MedianCut => {
                    WuQuantizer::quantize(opaque, requested)
                        .map_err(BitmapPaletteReductionError::Quantizer)?
                        .palette
                        .colors
                }
                BitmapPaletteReduction::Popularity => {
                    let mut bounded = options.clone();
                    bounded.maximum_colors = requested;
                    select_popularity_colors(histogram, original, &bounded)?
                }
            }
        };
        let Some(original) = original else {
            return Ok(colors);
        };
        let substituted = substitute_reusable_palette_colors(&mut colors, original, options)?;
        let free = options
            .entries
            .iter()
            .filter(|state| **state == BitmapPaletteEntryState::Free)
            .count();
        let unmatched = colors.len().saturating_sub(substituted);
        if unmatched <= free {
            return Ok(colors);
        }

        // ProcessBitmapGraphicsImport retries both reducers after preserved-color substitution.
        // Its next ceiling retains only the colors that actually found a reusable destination;
        // merely counting distinct reusable words can overestimate installable capacity.
        let next = requested.saturating_sub(unmatched - free).max(1);
        if next >= requested {
            return Ok(colors);
        }
        requested = next;
    }
}

fn native_reduction_color_limit(
    original: Option<&Palette>,
    options: &BitmapPaletteColorOptions,
) -> usize {
    let Some(original) = original else {
        return options.maximum_colors;
    };
    let free = options
        .entries
        .iter()
        .filter(|state| **state == BitmapPaletteEntryState::Free)
        .count();
    let mut reusable = options
        .entries
        .iter()
        .zip(&original.colors)
        .filter_map(|(state, color)| {
            (*state == BitmapPaletteEntryState::Reusable).then_some(color.0)
        })
        .collect::<Vec<_>>();
    reusable.sort_unstable();
    reusable.dedup();
    options
        .maximum_colors
        .min(free.saturating_add(reusable.len()).max(1))
}

fn nearest_lunar_magic_palette_indices(
    pixels: &[Rgb8],
    colors: &[Bgr555],
) -> Result<Vec<u8>, BitmapPaletteReductionError> {
    if colors.is_empty() {
        return Err(BitmapPaletteReductionError::EmptyOpaquePalette);
    }
    pixels
        .iter()
        .map(|pixel| {
            let source = lunar_magic_bitmap_color(*pixel).0;
            std::iter::once((0_usize, 0_u16))
                .chain(
                    colors
                        .iter()
                        .enumerate()
                        .map(|(index, color)| (index + 1, color.0)),
                )
                // An exact generated color wins a tie with the zero sentinel. Otherwise the
                // ordinary path retains Lunar Magic's zero fallback.
                .min_by_key(|(index, color)| {
                    (
                        lunar_magic_color_distance(source, *color),
                        *index == 0,
                        *index,
                    )
                })
                .and_then(|(index, _)| u8::try_from(index).ok())
                .ok_or(BitmapPaletteReductionError::IndexOverflow)
        })
        .collect()
}

fn collect_unique_available_palette_colors(
    original: &Palette,
    options: &BitmapPaletteColorOptions,
) -> Result<Vec<Bgr555>, BitmapPaletteReductionError> {
    if original.colors.len() < BITMAP_PALETTE_COLORS {
        return Err(BitmapPaletteReductionError::PaletteColors(
            original.colors.len(),
        ));
    }
    let mut colors = options
        .entries
        .iter()
        .zip(&original.colors)
        .filter_map(|(state, color)| {
            (*state != BitmapPaletteEntryState::Reserved).then_some(*color)
        })
        .collect::<Vec<_>>();
    let mut compare = 0;
    while compare < colors.len() {
        let mut candidate = compare + 1;
        let mut last = colors.len().saturating_sub(1);
        while candidate < colors.len() {
            if colors[compare] == colors[candidate] {
                colors[candidate] = colors[last];
                colors.pop();
                last = last.saturating_sub(1);
            }
            candidate += 1;
        }
        compare += 1;
    }
    if colors.is_empty() {
        colors.push(Bgr555(0));
    }
    Ok(colors)
}

fn substitute_reusable_palette_colors(
    colors: &mut [Bgr555],
    original: &Palette,
    options: &BitmapPaletteColorOptions,
) -> Result<usize, BitmapPaletteReductionError> {
    if original.colors.len() < BITMAP_PALETTE_COLORS {
        return Err(BitmapPaletteReductionError::PaletteColors(
            original.colors.len(),
        ));
    }
    if !options.entries.contains(&BitmapPaletteEntryState::Free) {
        return Ok(0);
    }
    let reusable = options
        .entries
        .iter()
        .zip(&original.colors)
        .enumerate()
        .filter_map(|(index, (state, color))| {
            (index % Palette::COLORS_PER_ROW != 0 && *state == BitmapPaletteEntryState::Reusable)
                .then_some(color.0)
        })
        .collect::<Vec<_>>();
    let mut selected_available = vec![true; colors.len()];
    let mut reusable_color_used = BTreeMap::<u16, ()>::new();
    let mut substitutions = 0;
    loop {
        let mut best = None::<(u32, usize, usize)>;
        for (selected_index, selected) in colors.iter().enumerate() {
            if !selected_available[selected_index] {
                continue;
            }
            for (reusable_index, candidate) in reusable.iter().enumerate() {
                if reusable_color_used.contains_key(candidate) {
                    continue;
                }
                let proposal = (
                    lunar_magic_color_distance(selected.0, *candidate),
                    selected_index,
                    reusable_index,
                );
                if best.is_none_or(|current| proposal < current) {
                    best = Some(proposal);
                }
            }
        }
        let Some((_, selected_index, reusable_index)) = best else {
            break;
        };
        selected_available[selected_index] = false;
        let candidate = reusable[reusable_index];
        if reusable_color_matches(
            colors[selected_index].0,
            candidate,
            options.reusable_color_hue_tolerance,
        ) {
            colors[selected_index] = Bgr555(candidate);
            reusable_color_used.insert(candidate, ());
            substitutions += 1;
        }
    }
    Ok(substitutions)
}

fn reusable_color_matches(selected: u16, reusable: u16, hue_tolerance: u16) -> bool {
    if hue_tolerance >= 240 {
        return true;
    }
    let selected = lunar_magic_hsl240(selected);
    let reusable = lunar_magic_hsl240(reusable);
    if selected.saturation < 31 && reusable.saturation < 31 {
        return true;
    }
    if circular_hue_distance(selected.hue, reusable.hue) > hue_tolerance {
        return false;
    }
    if selected.lightness > 15 && reusable.lightness > 15 {
        return selected.saturation > 30 && reusable.saturation > 30;
    }
    selected.lightness < 16
        && reusable.lightness < 16
        && selected.saturation.abs_diff(reusable.saturation) < 60
}

fn maintain_detail_palette_indices(
    opaque: &[Rgb8],
    colors: &[Bgr555],
    exact_existing: &[u16],
) -> Result<Vec<u8>, BitmapPaletteReductionError> {
    if colors.is_empty() {
        return Err(BitmapPaletteReductionError::EmptyOpaquePalette);
    }
    let mut sources = opaque
        .iter()
        .map(|pixel| lunar_magic_bitmap_color(*pixel).0)
        .collect::<Vec<_>>();
    sources.sort_unstable();
    sources.dedup();
    let candidates = std::iter::once(0_u16)
        .chain(colors.iter().map(|color| color.0))
        .collect::<Vec<_>>();
    let mut source_assignments = BTreeMap::<u16, u8>::new();
    let mut palette_assigned = vec![false; candidates.len()];
    for (palette_index, color) in candidates.iter().copied().enumerate() {
        if palette_index == 0 && exact_existing.binary_search(&color).is_ok() {
            continue;
        }
        if sources.binary_search(&color).is_ok() && !source_assignments.contains_key(&color) {
            source_assignments.insert(
                color,
                u8::try_from(palette_index)
                    .map_err(|_| BitmapPaletteReductionError::IndexOverflow)?,
            );
            palette_assigned[palette_index] = true;
        }
    }
    loop {
        let mut best = None::<(u32, usize, u16)>;
        for (palette_index, color) in candidates.iter().copied().enumerate() {
            if palette_assigned[palette_index] {
                continue;
            }
            for source in &sources {
                if source_assignments.contains_key(source) {
                    continue;
                }
                let candidate = (
                    lunar_magic_color_distance(color, *source),
                    palette_index,
                    *source,
                );
                if best.is_none_or(|current| candidate < current) {
                    best = Some(candidate);
                }
            }
        }
        let Some((_, palette_index, source)) = best else {
            break;
        };
        source_assignments.insert(
            source,
            u8::try_from(palette_index).map_err(|_| BitmapPaletteReductionError::IndexOverflow)?,
        );
        palette_assigned[palette_index] = true;
    }
    for source in sources {
        source_assignments.entry(source).or_insert_with(|| {
            candidates
                .iter()
                .copied()
                .enumerate()
                .min_by_key(|(palette_index, color)| {
                    (lunar_magic_color_distance(source, *color), *palette_index)
                })
                .and_then(|(palette_index, _)| u8::try_from(palette_index).ok())
                .unwrap_or(0)
        });
    }
    opaque
        .iter()
        .map(|pixel| {
            let source = lunar_magic_bitmap_color(*pixel).0;
            source_assignments
                .get(&source)
                .copied()
                .ok_or(BitmapPaletteReductionError::IndexOverflow)
        })
        .collect()
}

const fn lunar_magic_bitmap_channel(channel: u8) -> u16 {
    let truncated = channel & 0xf8;
    let rounded = if channel & 4 != 0 && truncated < 0xf8 {
        truncated + 8
    } else {
        truncated
    };
    (rounded >> 3) as u16
}

const fn lunar_magic_bitmap_color(color: Rgb8) -> Bgr555 {
    Bgr555(
        lunar_magic_bitmap_channel(color.red)
            | (lunar_magic_bitmap_channel(color.green) << 5)
            | (lunar_magic_bitmap_channel(color.blue) << 10),
    )
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
        let nearest = if options.prioritize_unique_colors {
            reusable
                .iter()
                .chain(selected.iter().map(|(selected, _)| selected))
                .map(|candidate| lunar_magic_color_distance(color, *candidate))
                .min()
        } else {
            None
        };
        if let Some(mut distance) = nearest {
            for _ in 1..options.priority_level {
                distance = distance.wrapping_mul(distance);
            }
            score = score.wrapping_add(distance.wrapping_mul(frequency).wrapping_div(0x8e_e09));
        }
        if options.popularity_reduction_method_1
            && apply_popularity_reduction_method_1(&mut selected, color, score)
        {
            continue;
        }
        if options.popularity_reduction_method_2
            && apply_popularity_reduction_method_2(&mut selected, color, score)
        {
            continue;
        }
        let insertion = selected.partition_point(|(_, selected_score)| *selected_score >= score);
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

fn apply_popularity_reduction_method_1(
    selected: &mut [(u16, u32)],
    color: u16,
    score: u32,
) -> bool {
    let red_start = component_range_start(color & 0x1f, 1);
    let green_start = component_range_start((color >> 5) & 0x1f, 1);
    let blue_start = component_range_start(color >> 10, 1);
    let red_end = red_start.wrapping_add(3).min(32);
    let green_end = green_start.wrapping_add(3).min(32);
    let blue_end = blue_start.wrapping_add(3).min(32);

    let mut found = false;
    'neighborhood: for red in red_start..red_end {
        for green in green_start..green_end {
            for blue in blue_start..blue_end {
                let neighbor = (blue << 10) | (green << 5) | red;
                let Some(index) = selected.iter().position(|(candidate, candidate_score)| {
                    *candidate_score != 0 && *candidate == neighbor
                }) else {
                    continue;
                };
                if score > selected[index].1 {
                    selected[index] = (color, score);
                    bubble_popularity_color_up(selected, index, score);
                    found = true;
                    // A replacement restores every loop counter to its end bound and exits the
                    // complete neighborhood scan.
                    break 'neighborhood;
                }
                found = true;
            }
        }
    }
    found
}

fn apply_popularity_reduction_method_2(
    selected: &mut [(u16, u32)],
    color: u16,
    score: u32,
) -> bool {
    let red_start = component_range_start(color & 0x1f, 2);
    let green_start = component_range_start((color >> 5) & 0x1f, 1);
    let blue_start = component_range_start(color >> 10, 1);
    let red_end = red_start.wrapping_add(5).min(32);
    let green_end = green_start.wrapping_add(4).min(32);
    let blue_end = blue_start.wrapping_add(3).min(32);
    let mut found = false;
    let mut weakest = None::<(usize, u32)>;

    for red in red_start..red_end {
        for green in green_start..green_end {
            for blue in blue_start..blue_end {
                let neighbor = (blue << 10) | (green << 5) | red;
                let Some((index, candidate_score)) = selected.iter().enumerate().find_map(
                    |(index, (candidate, candidate_score))| {
                        (*candidate_score != 0 && *candidate == neighbor)
                            .then_some((index, *candidate_score))
                    },
                ) else {
                    continue;
                };
                found = true;
                if candidate_score >= score {
                    // A strong neighbor cancels aggregation and exits the complete scan.
                    return true;
                } else if weakest.is_none_or(|(_, weakest_score)| candidate_score < weakest_score) {
                    weakest = Some((index, candidate_score));
                }
            }
        }
    }

    if let Some((index, weakest_score)) = weakest
        && weakest_score < 0x80
    {
        selected[index] = (color, weakest_score.wrapping_add(score));
        /*
         * The original compares preceding entries against the incoming score,
         * not the combined score. If this entry moves, its stored score also
         * becomes the incoming score.
         */
        bubble_popularity_color_up(selected, index, score);
    }
    found
}

const fn component_range_start(component: u16, radius: u16) -> u16 {
    component.wrapping_sub(radius)
}

fn bubble_popularity_color_up(selected: &mut [(u16, u32)], mut index: usize, score: u32) {
    let color = selected[index].0;
    while index > 0 && score > selected[index - 1].1 {
        selected[index] = selected[index - 1];
        selected[index - 1] = (color, score);
        index -= 1;
    }
}

/// Assigns globally reduced source colors to Lunar Magic's eight 16-color rows.
///
/// Source dimensions must be complete 8×8 tiles. Unique tile color sets retain direct pixel weights
/// plus aggregate weights from still-unassigned strict subsets. Each native exact-fit iteration
/// chooses a capacity seed, combines overlapping records, then installs that proposal into the
/// globally best target row; overlap, capacity, direct occurrence weight, set length, and first
/// tile occurrence break the corresponding ties. The later partial-set pass chooses records by overlap and aggregate
/// weight, but installs their strongest missing colors by direct weight. The final pass independently
/// scores every 8×8 tile against each usable row with Lunar Magic's weighted RGB555 distance,
/// selects the least-error row, and converts its pixels to that row's nearest entries. A source color
/// therefore need not have been installed exactly.
///
/// Reusable colors retain their exact palette indexes. Reserved entries are neither overwritten
/// nor candidates. Entry zero remains transparency-only. Tiles that exceed the usable native row
/// capacity are reduced with Lunar Magic's border-weighted local pass before allocation.
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
    let mut rows: [PaletteRowAllocation; BITMAP_PALETTE_ROWS] =
        std::array::from_fn(|row| PaletteRowAllocation::new(row, original, options));
    let tiles_wide = width / 8;
    let tiles_high = height / 8;
    let mut reduced = reduced.clone();
    reduce_tiles_to_native_row_capacity(
        &mut reduced,
        width,
        height,
        tiles_wide,
        tiles_high,
        &rows,
    )?;
    let tile_sets = build_tile_color_sets(&reduced, width, tiles_wide, tiles_high)?;
    let mut records = build_color_set_records(&tile_sets);
    if options.prioritize_exact_palette_matches && options.allow_modifying_unmarked_colors {
        retain_exact_existing_color_sets(&mut records, &mut rows);
        for row in &mut rows {
            row.discard_unclaimed_free_colors();
        }
    }
    if options.allow_modifying_unmarked_colors {
        assign_color_set_records(&mut records, &mut rows)?;
    } else {
        // The existing-colors-only preprocessing path has already claimed matching free words.
        // Preserve its separate retained-row traversal before the native revisit extension below.
        assign_existing_color_set_records(&mut records, &mut rows)?;
    }
    if !options.maintain_detail {
        extend_palette_rows_with_weighted_colors(
            &mut records,
            &mut rows,
            !options.allow_modifying_unmarked_colors,
        )?;
    }
    for row in &mut rows {
        row.order_assigned_colors(!options.allow_modifying_unmarked_colors);
    }
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
    let (indices, tile_rows) = assign_tiles_to_lowest_error_rows(
        &reduced,
        width,
        tiles_wide,
        tiles_high,
        &rows,
        !options.allow_modifying_unmarked_colors,
    )?;
    Ok(MultiRowBitmapPalette {
        palette,
        indices,
        tile_rows,
        generated_colors,
    })
}

fn retain_exact_existing_color_sets(
    records: &mut BTreeMap<Vec<u16>, ColorSetRecord>,
    rows: &mut [PaletteRowAllocation; BITMAP_PALETTE_ROWS],
) {
    for record in records.values_mut() {
        if record.assigned_row.is_some() || record.colors.is_empty() {
            continue;
        }
        if record.colors.iter().any(|color| *color != 0) {
            continue;
        }
        let Some(row) = rows.iter().position(|row| {
            record.colors.iter().all(|color| {
                row.entries
                    .iter()
                    .any(|entry| matches!(entry, RowEntry::Free(Some(value)) if value == color))
            })
        }) else {
            continue;
        };
        rows[row].claim_exact_free_colors(&record.colors);
        record.assigned_row = Some(row);
    }
    recompute_color_set_aggregate_weights(records);
}

fn reduce_tiles_to_native_row_capacity(
    reduced: &mut ReducedBitmapPalette,
    width: usize,
    height: usize,
    tiles_wide: usize,
    tiles_high: usize,
    rows: &[PaletteRowAllocation; BITMAP_PALETTE_ROWS],
) -> Result<(), BitmapPaletteReductionError> {
    let Some((capacity, special_color)) = native_tile_color_capacity(rows) else {
        return Ok(());
    };
    let color_indices = reduced
        .colors
        .iter()
        .enumerate()
        .map(|(index, color)| (color.0, u8::try_from(index + 1).unwrap_or(u8::MAX)))
        .collect::<BTreeMap<_, _>>();

    for tile_y in 0..tiles_high {
        for tile_x in 0..tiles_wide {
            let mut histogram = tile_color_histogram(reduced, width, tile_x, tile_y)?;
            let mut target = capacity;
            let requires_reduction = if histogram.colors.len() > capacity {
                if let Some(special) = special_color {
                    match histogram.colors.binary_search(&special) {
                        Ok(index) if histogram.weights[index] > 2 => {
                            histogram.weights[index] += 0x80;
                        }
                        _ => target = target.saturating_sub(1),
                    }
                }
                true
            } else if histogram.colors.len() == capacity && special_color.is_some() {
                if histogram
                    .colors
                    .binary_search(&special_color.expect("checked above"))
                    .is_err()
                {
                    target = target.saturating_sub(1);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if !requires_reduction {
                continue;
            }

            boost_tile_border_colors(&mut histogram, reduced, width, height, tile_x, tile_y)?;
            let selected = strongest_tile_colors(&histogram, target);
            if selected.is_empty() {
                continue;
            }
            for pixel_y in 0..8 {
                let row = (tile_y * 8 + pixel_y) * width + tile_x * 8;
                for index in &mut reduced.indices[row..row + 8] {
                    if *index == 0 {
                        continue;
                    }
                    let source = reduced
                        .colors
                        .get(usize::from(*index) - 1)
                        .ok_or(BitmapPaletteReductionError::ReducedIndex(*index))?
                        .0;
                    let replacement = selected
                        .iter()
                        .copied()
                        .enumerate()
                        .min_by_key(|(order, color)| {
                            (lunar_magic_color_distance(source, *color), *order)
                        })
                        .map(|(_, color)| color)
                        .expect("a nonempty selected palette has a nearest color");
                    *index = color_indices[&replacement];
                }
            }
        }
    }
    Ok(())
}

fn native_tile_color_capacity(
    rows: &[PaletteRowAllocation; BITMAP_PALETTE_ROWS],
) -> Option<(usize, Option<u16>)> {
    let mut best: Option<(usize, Option<u16>)> = None;
    for row in rows {
        let free = row.free_count();
        if free == 0 {
            continue;
        }
        let first_reusable = (free > 1).then_some(row.first_reusable_color).flatten();
        if best.is_none_or(|(best_free, best_special)| {
            free > best_free || (free == best_free && best_special.is_some())
        }) {
            best = Some((free, first_reusable));
        }
    }
    best
}

fn tile_color_histogram(
    reduced: &ReducedBitmapPalette,
    width: usize,
    tile_x: usize,
    tile_y: usize,
) -> Result<TileColorHistogram, BitmapPaletteReductionError> {
    let mut histogram = BTreeMap::<u16, usize>::new();
    for pixel_y in 0..8 {
        let row = (tile_y * 8 + pixel_y) * width + tile_x * 8;
        for index in &reduced.indices[row..row + 8] {
            if *index == 0 {
                continue;
            }
            let color = reduced
                .colors
                .get(usize::from(*index) - 1)
                .ok_or(BitmapPaletteReductionError::ReducedIndex(*index))?;
            *histogram.entry(color.0).or_default() += 1;
        }
    }
    let (colors, weights) = histogram.into_iter().unzip();
    Ok(TileColorHistogram { colors, weights })
}

fn boost_tile_border_colors(
    histogram: &mut TileColorHistogram,
    reduced: &ReducedBitmapPalette,
    width: usize,
    height: usize,
    tile_x: usize,
    tile_y: usize,
) -> Result<(), BitmapPaletteReductionError> {
    let x = tile_x * 8;
    let y = tile_y * 8;
    let mut boost = |offset: usize| -> Result<(), BitmapPaletteReductionError> {
        let index = reduced.indices[offset];
        if index == 0 {
            return Ok(());
        }
        let color = reduced
            .colors
            .get(usize::from(index) - 1)
            .ok_or(BitmapPaletteReductionError::ReducedIndex(index))?
            .0;
        if let Ok(position) = histogram.colors.binary_search(&color)
            && histogram.weights[position] > 2
        {
            histogram.weights[position] += 1;
        }
        Ok(())
    };
    if y != 0 {
        for pixel_x in 0..8 {
            boost((y - 1) * width + x + pixel_x)?;
        }
    }
    if y + 8 < height {
        for pixel_x in 0..8 {
            boost((y + 8) * width + x + pixel_x)?;
        }
    }
    if x != 0 {
        for pixel_y in 0..8 {
            boost((y + pixel_y) * width + x - 1)?;
        }
    }
    if x + 8 < width {
        for pixel_y in 0..8 {
            boost((y + pixel_y) * width + x + 8)?;
        }
    }
    Ok(())
}

fn strongest_tile_colors(histogram: &TileColorHistogram, target: usize) -> Vec<u16> {
    let mut ranked = histogram
        .colors
        .iter()
        .copied()
        .zip(histogram.weights.iter().copied())
        .enumerate()
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(order, (_, weight))| (Reverse(*weight), *order));
    ranked
        .into_iter()
        .take(target)
        .map(|(_, (color, _))| color)
        .collect()
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
            let histogram = tile_color_histogram(reduced, width, tile_x, tile_y)?;
            if histogram.colors.len() > 15 {
                return Err(BitmapPaletteReductionError::TileColors {
                    tile: tile_sets.len(),
                    colors: histogram.colors.len(),
                });
            }
            tile_sets.push(histogram);
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
    recompute_color_set_aggregate_weights(&mut records);
    records
}

fn recompute_color_set_aggregate_weights(records: &mut BTreeMap<Vec<u16>, ColorSetRecord>) {
    let keys = records.keys().cloned().collect::<Vec<_>>();
    let weights = keys
        .iter()
        .map(|key| {
            let mut aggregate = records[key].direct_weights.clone();
            for subset in keys.iter().filter(|subset| {
                subset.len() < key.len()
                    && records[*subset].assigned_row.is_none()
                    && is_subset(subset, key)
            }) {
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
}

// Keeping the recovered seed, proposal, target, and coverage phases together makes their unusual
// row-zero sentinel and temporary coverage state auditable against the original listing.
#[allow(clippy::too_many_lines)]
fn assign_color_set_records(
    records: &mut BTreeMap<Vec<u16>, ColorSetRecord>,
    rows: &mut [PaletteRowAllocation; BITMAP_PALETTE_ROWS],
) -> Result<(), BitmapPaletteReductionError> {
    if let Some(empty) = records.get_mut(&Vec::new()) {
        empty.assigned_row = Some(0);
    }
    let mut active: [bool; BITMAP_PALETTE_ROWS] =
        std::array::from_fn(|row| rows[row].free_count() != 0);
    while records
        .values()
        .any(|record| record.assigned_row.is_none() && !record.colors.is_empty())
        && active.iter().any(|active| *active)
    {
        // The original first chooses a row whose reusable colors and remaining capacity seed
        // SelectPaletteColorSetsForCapacity. The combined set returned by that helper is then
        // offered to every row; it is not necessarily installed in the seed row.
        let seed_row = (0..rows.len())
            .filter(|row| active[*row])
            .max_by_key(|row| (rows[*row].reusable_count(), rows[*row].free_count()))
            .expect("an active palette row exists");
        let mut selected = rows[seed_row].colors();
        let mut capacity = rows[seed_row].free_count();
        let mut seeded = false;
        loop {
            let next = records
                .values()
                .filter_map(|record| {
                    if record.assigned_row.is_some() || record.colors.is_empty() {
                        return None;
                    }
                    let overlap = record
                        .colors
                        .iter()
                        .filter(|color| selected.binary_search(color).is_ok())
                        .count();
                    let missing = record.colors.len() - overlap;
                    if missing > capacity {
                        return None;
                    }
                    let score = RowScore { overlap };
                    (!seeded || score.overlap != 0).then_some((record, score))
                })
                .max_by(|(left, left_score), (right, right_score)| {
                    left_score
                        .overlap
                        .cmp(&right_score.overlap)
                        .then_with(|| {
                            left.direct_weights
                                .iter()
                                .sum::<usize>()
                                .cmp(&right.direct_weights.iter().sum::<usize>())
                        })
                        .then_with(|| left.colors.len().cmp(&right.colors.len()))
                        .then_with(|| right.tiles[0].cmp(&left.tiles[0]))
                })
                .map(|(record, _)| record.colors.clone());
            let Some(colors) = next else {
                break;
            };
            seeded = true;
            for color in &colors {
                if selected.binary_search(color).is_err() {
                    let index = selected.partition_point(|candidate| candidate < color);
                    selected.insert(index, *color);
                    capacity -= 1;
                }
            }
            for record in records.values_mut() {
                if record.assigned_row.is_none() && is_subset(&record.colors, &selected) {
                    // A temporary out-of-range row reproduces the native covered flag while the
                    // seed helper aggregates more records into this one proposal.
                    record.assigned_row = Some(BITMAP_PALETTE_ROWS);
                }
            }
            recompute_color_set_aggregate_weights(records);
        }
        for record in records.values_mut() {
            if record.assigned_row == Some(BITMAP_PALETTE_ROWS) {
                record.assigned_row = None;
            }
        }
        recompute_color_set_aggregate_weights(records);
        if !seeded {
            active[seed_row] = false;
            continue;
        }

        let mut target = None::<(usize, RowScore)>;
        for row in 0..rows.len() {
            if !active[row] {
                continue;
            }
            let Some(score) = rows[row].score(&selected) else {
                continue;
            };
            let replace = target.is_none_or(|(best_row, best_score)| {
                best_row == 0
                    || score.overlap > best_score.overlap
                    || (score.overlap == best_score.overlap
                        && rows[row].free_count() < rows[best_row].free_count())
            });
            if replace {
                target = Some((row, score));
            }
        }
        let Some((row, _)) = target else {
            active[seed_row] = false;
            continue;
        };
        rows[row].install(&selected, true)?;
        let covered = rows[row].colors();
        for record in records.values_mut() {
            if record.assigned_row.is_none() && is_subset(&record.colors, &covered) {
                record.assigned_row = Some(row);
            }
        }
        recompute_color_set_aggregate_weights(records);
    }
    Ok(())
}

fn assign_existing_color_set_records(
    records: &mut BTreeMap<Vec<u16>, ColorSetRecord>,
    rows: &mut [PaletteRowAllocation; BITMAP_PALETTE_ROWS],
) -> Result<(), BitmapPaletteReductionError> {
    if let Some(empty) = records.get_mut(&Vec::new()) {
        empty.assigned_row = Some(0);
    }
    let mut row_order = (0..rows.len()).collect::<Vec<_>>();
    row_order.sort_by_key(|row| {
        (
            Reverse(rows[*row].reusable_count()),
            rows[*row].free_count(),
            *row,
        )
    });
    for row in row_order {
        let mut seeded = false;
        loop {
            let next = records
                .values()
                .filter_map(|record| {
                    if record.assigned_row.is_some() || record.colors.is_empty() {
                        return None;
                    }
                    let score = rows[row].score(&record.colors)?;
                    (!seeded || score.overlap != 0).then_some((record, score))
                })
                .max_by(|(left, left_score), (right, right_score)| {
                    left_score
                        .overlap
                        .cmp(&right_score.overlap)
                        .then_with(|| {
                            left.direct_weights
                                .iter()
                                .sum::<usize>()
                                .cmp(&right.direct_weights.iter().sum::<usize>())
                        })
                        .then_with(|| left.colors.len().cmp(&right.colors.len()))
                        .then_with(|| right.tiles[0].cmp(&left.tiles[0]))
                })
                .map(|(record, _)| record.colors.clone());
            let Some(colors) = next else {
                break;
            };
            rows[row].install(&colors, true)?;
            seeded = true;
            let covered = rows[row].colors();
            for record in records.values_mut() {
                if record.assigned_row.is_none() && is_subset(&record.colors, &covered) {
                    record.assigned_row = Some(row);
                }
            }
            recompute_color_set_aggregate_weights(records);
        }
    }
    Ok(())
}

fn extend_palette_rows_with_weighted_colors(
    records: &mut BTreeMap<Vec<u16>, ColorSetRecord>,
    rows: &mut [PaletteRowAllocation; BITMAP_PALETTE_ROWS],
    revisit_assigned_records: bool,
) -> Result<(), BitmapPaletteReductionError> {
    let mut row_order = (0..rows.len()).collect::<Vec<_>>();
    row_order.sort_by_key(|row| {
        (
            Reverse(rows[*row].reusable_count()),
            rows[*row].free_count(),
            *row,
        )
    });
    for row in row_order {
        loop {
            let capacity = rows[row].free_count();
            if capacity == 0 {
                break;
            }
            let existing = rows[row].colors();
            let next = records
                .values()
                .filter(|record| {
                    (revisit_assigned_records || record.assigned_row.is_none())
                        && record
                            .colors
                            .iter()
                            .any(|color| existing.binary_search(color).is_err())
                })
                .max_by(|left, right| {
                    let left_overlap = left
                        .colors
                        .iter()
                        .filter(|color| existing.binary_search(color).is_ok())
                        .count();
                    let right_overlap = right
                        .colors
                        .iter()
                        .filter(|color| existing.binary_search(color).is_ok())
                        .count();
                    left_overlap
                        .cmp(&right_overlap)
                        .then_with(|| left.aggregate_weight.cmp(&right.aggregate_weight))
                        .then_with(|| compare_color_set_priority(left, right))
                })
                .map(|record| record.colors.clone());
            let Some(colors) = next else {
                break;
            };
            let record = records
                .get(&colors)
                .ok_or_else(|| BitmapPaletteReductionError::UnassignedColorSet(colors.clone()))?;
            let mut missing = record
                .colors
                .iter()
                .copied()
                .zip(record.direct_weights.iter().copied())
                .filter(|(color, _)| existing.binary_search(color).is_err())
                .collect::<Vec<_>>();
            missing.sort_by_key(|(color, weight)| (Reverse(*weight), *color));
            let selected = missing
                .into_iter()
                .take(capacity)
                .map(|(color, _)| color)
                .collect::<Vec<_>>();
            if selected.is_empty() {
                break;
            }
            rows[row].install(&selected, !revisit_assigned_records)?;
            let covered = rows[row].colors();
            for record in records.values_mut() {
                if record.assigned_row.is_none() && is_subset(&record.colors, &covered) {
                    record.assigned_row = Some(row);
                }
            }
            if let Some(record) = records.get_mut(&colors) {
                record.assigned_row = Some(row);
            }
            recompute_color_set_aggregate_weights(records);
        }
    }
    Ok(())
}

fn assign_tiles_to_lowest_error_rows(
    reduced: &ReducedBitmapPalette,
    width: usize,
    tiles_wide: usize,
    tiles_high: usize,
    rows: &[PaletteRowAllocation; BITMAP_PALETTE_ROWS],
    prefer_assigned_entries: bool,
) -> Result<(Vec<u8>, Vec<u8>), BitmapPaletteReductionError> {
    let mut indices = vec![0; reduced.indices.len()];
    let mut tile_rows = Vec::with_capacity(tiles_wide * tiles_high);
    for tile_y in 0..tiles_high {
        for tile_x in 0..tiles_wide {
            let mut best: Option<(u64, usize, usize, [u8; 64])> = None;
            for row in rows {
                let Some((error, assigned_pixels, tile_indices)) = score_tile_for_row(
                    reduced,
                    width,
                    tile_x,
                    tile_y,
                    row,
                    prefer_assigned_entries,
                )?
                else {
                    continue;
                };
                let assigned_priority = if prefer_assigned_entries {
                    assigned_pixels
                } else {
                    0
                };
                if best
                    .as_ref()
                    .is_none_or(|(best_error, best_assigned, best_row, _)| {
                        (error, Reverse(assigned_priority), row.row)
                            < (*best_error, Reverse(*best_assigned), *best_row)
                    })
                {
                    best = Some((error, assigned_priority, row.row, tile_indices));
                }
            }
            let Some((_, _, row, tile_indices)) = best else {
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
    prefer_last_equal_entry: bool,
) -> Result<Option<(u64, usize, [u8; 64])>, BitmapPaletteReductionError> {
    let mut error = 0_u64;
    let mut assigned_pixels = 0;
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
            let Some((entry, distance)) = row.nearest(color, prefer_last_equal_entry) else {
                return Ok(None);
            };
            indices[pixel_y * 8 + pixel_x] = entry;
            error = error.saturating_add(u64::from(distance));
            if distance == 0 && matches!(row.entries[usize::from(entry)], RowEntry::Assigned(_)) {
                assigned_pixels += 1;
            }
        }
    }
    Ok(Some((error, assigned_pixels, indices)))
}

fn lunar_magic_color_distance(left: u16, right: u16) -> u32 {
    let red = i32::from(left & 0x1f) - i32::from(right & 0x1f);
    let green = i32::from((left >> 5) & 0x1f) - i32::from((right >> 5) & 0x1f);
    let blue = i32::from((left >> 10) & 0x1f) - i32::from((right >> 10) & 0x1f);
    u32::try_from(red * red * 4 + green * green * 3 + blue * blue * 2).unwrap_or(u32::MAX)
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
        .then_with(|| left.colors.len().cmp(&right.colors.len()))
        .then_with(|| right.tiles[0].cmp(&left.tiles[0]))
}

fn is_subset(subset: &[u16], superset: &[u16]) -> bool {
    subset
        .iter()
        .all(|color| superset.binary_search(color).is_ok())
}

#[derive(Clone, Copy, Debug)]
enum RowEntry {
    Reserved,
    Free(Option<u16>),
    Reusable(u16),
    Retained(u16),
    Exact(u16),
    Assigned(u16),
}

#[derive(Debug)]
struct PaletteRowAllocation {
    row: usize,
    entries: [RowEntry; Palette::COLORS_PER_ROW],
    first_reusable_color: Option<u16>,
}

#[derive(Clone, Copy)]
struct RowScore {
    overlap: usize,
}

impl PaletteRowAllocation {
    fn claim_exact_free_colors(&mut self, colors: &[u16]) {
        for color in colors {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|entry| matches!(entry, RowEntry::Free(Some(value)) if value == color))
            {
                *entry = RowEntry::Exact(*color);
            }
        }
    }

    fn discard_unclaimed_free_colors(&mut self) {
        for entry in &mut self.entries {
            if matches!(entry, RowEntry::Free(Some(_))) {
                *entry = RowEntry::Free(None);
            }
        }
    }

    fn new(row: usize, original: &Palette, options: &BitmapPaletteColorOptions) -> Self {
        let first = row * Palette::COLORS_PER_ROW;
        let first_reusable_color = (options.entries[first] == BitmapPaletteEntryState::Reusable)
            .then_some(original.colors[first].0);
        let entries = std::array::from_fn(|entry| {
            let index = row * Palette::COLORS_PER_ROW + entry;
            match options.entries[index] {
                // Retain the live value until the black exact-match prepass is complete. The
                // ordinary modifiable path clears every unclaimed value before row allocation.
                BitmapPaletteEntryState::Free if entry != 0 => {
                    RowEntry::Free(Some(original.colors[index].0))
                }
                BitmapPaletteEntryState::Reusable if entry != 0 => {
                    RowEntry::Reusable(original.colors[index].0)
                }
                BitmapPaletteEntryState::Free
                | BitmapPaletteEntryState::Reusable
                | BitmapPaletteEntryState::Reserved => RowEntry::Reserved,
            }
        });
        Self {
            row,
            entries,
            first_reusable_color,
        }
    }

    fn score(&self, colors: &[u16]) -> Option<RowScore> {
        let overlap = colors
            .iter()
            .filter(|color| self.index_of(**color).is_some())
            .count();
        let free_before = self
            .entries
            .iter()
            .filter(|entry| matches!(entry, RowEntry::Free(_)))
            .count();
        (colors.len() - overlap <= free_before).then_some(RowScore { overlap })
    }

    fn free_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, RowEntry::Free(_)))
            .count()
    }

    fn reusable_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| matches!(entry, RowEntry::Reusable(_)))
            .count()
    }

    fn install(
        &mut self,
        colors: &[u16],
        retain_matching_free_colors: bool,
    ) -> Result<(), BitmapPaletteReductionError> {
        if retain_matching_free_colors {
            for color in colors {
                if self.installed_index_of(*color).is_some() {
                    continue;
                }
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|entry| matches!(entry, RowEntry::Free(Some(value)) if value == color))
                {
                    *entry = RowEntry::Retained(*color);
                }
            }
        }
        for color in colors {
            if self.installed_index_of(*color).is_some() {
                continue;
            }
            let entry = self
                .entries
                .iter_mut()
                .find(|entry| matches!(entry, RowEntry::Free(_)))
                .ok_or(BitmapPaletteReductionError::RowCapacity(self.row))?;
            *entry = RowEntry::Assigned(*color);
        }
        Ok(())
    }

    fn order_assigned_colors(&mut self, preserve_first: bool) {
        let first = preserve_first
            .then(|| {
                self.entries
                    .iter()
                    .position(|entry| assigned_color(*entry).is_some())
            })
            .flatten();
        let mut previous = first.and_then(|entry| assigned_color(self.entries[entry]));
        let mut entry = first.map_or(0, |entry| entry + 1);
        while entry < self.entries.len() {
            if !matches!(self.entries[entry], RowEntry::Assigned(_)) {
                entry += 1;
                continue;
            }
            let candidates = entry..self.entries.len();
            let selected = if let Some(previous_color) = previous {
                candidates
                    .clone()
                    .filter_map(|candidate| {
                        assigned_color(self.entries[candidate]).map(|color| {
                            (
                                hsl_ordering_distance(previous_color, color),
                                candidate,
                                color,
                            )
                        })
                    })
                    .min_by_key(|(distance, candidate, _)| (*distance, *candidate))
            } else {
                candidates
                    .clone()
                    .filter_map(|candidate| {
                        assigned_color(self.entries[candidate]).map(|color| {
                            (
                                u32::from(lunar_magic_hsl240(color).lightness),
                                candidate,
                                color,
                            )
                        })
                    })
                    .min_by_key(|(lightness, candidate, _)| (*lightness, *candidate))
            };
            let Some((_, selected, color)) = selected else {
                entry += 1;
                continue;
            };
            if let Some(previous_color) = previous {
                let previous_hsl = lunar_magic_hsl240(previous_color);
                let selected_hsl = lunar_magic_hsl240(color);
                if (previous_hsl.lightness > 15 || selected_hsl.lightness > 15)
                    && circular_hue_distance(previous_hsl.hue, selected_hsl.hue) > 45
                {
                    previous = None;
                    continue;
                }
            }
            self.entries.swap(entry, selected);
            previous = Some(color);
            entry += 1;
        }
    }

    fn index_of(&self, color: u16) -> Option<u8> {
        self.entries
            .iter()
            .position(|entry| {
                matches!(entry, RowEntry::Free(Some(value)) | RowEntry::Reusable(value) | RowEntry::Retained(value) | RowEntry::Exact(value) | RowEntry::Assigned(value) if *value == color)
            })
            .and_then(|entry| u8::try_from(entry).ok())
    }

    fn installed_index_of(&self, color: u16) -> Option<u8> {
        self.entries
            .iter()
            .position(|entry| {
                matches!(entry, RowEntry::Reusable(value) | RowEntry::Retained(value) | RowEntry::Exact(value) | RowEntry::Assigned(value) if *value == color)
            })
            .and_then(|entry| u8::try_from(entry).ok())
    }

    fn nearest(&self, color: u16, prefer_last_equal_entry: bool) -> Option<(u8, u32)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(entry, candidate)| match candidate {
                RowEntry::Free(Some(value))
                | RowEntry::Reusable(value)
                | RowEntry::Retained(value)
                | RowEntry::Exact(value)
                | RowEntry::Assigned(value) => {
                    let index = u8::try_from(entry).ok()?;
                    Some((index, lunar_magic_color_distance(color, *value)))
                }
                RowEntry::Reserved | RowEntry::Free(None) => None,
            })
            .min_by_key(|(entry, distance)| {
                (
                    *distance,
                    if prefer_last_equal_entry {
                        Reverse(*entry)
                    } else {
                        Reverse(0)
                    },
                    if prefer_last_equal_entry { 0 } else { *entry },
                )
            })
    }

    fn colors(&self) -> Vec<u16> {
        let mut colors = self
            .entries
            .iter()
            .filter_map(|entry| match entry {
                RowEntry::Reusable(color)
                | RowEntry::Retained(color)
                | RowEntry::Assigned(color) => Some(*color),
                RowEntry::Reserved | RowEntry::Free(_) | RowEntry::Exact(_) => None,
            })
            .collect::<Vec<_>>();
        colors.sort_unstable();
        colors.dedup();
        colors
    }
}

const fn assigned_color(entry: RowEntry) -> Option<u16> {
    match entry {
        RowEntry::Assigned(color) => Some(color),
        RowEntry::Reserved
        | RowEntry::Free(_)
        | RowEntry::Reusable(_)
        | RowEntry::Retained(_)
        | RowEntry::Exact(_) => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Hsl240 {
    hue: u16,
    saturation: u16,
    lightness: u16,
}

fn lunar_magic_hsl240(color: u16) -> Hsl240 {
    let rgb = Bgr555(color).to_rgb8();
    let red = u16::from(rgb.red);
    let green = u16::from(rgb.green);
    let blue = u16::from(rgb.blue);
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let sum = maximum + minimum;
    let lightness = (u32::from(sum) * 240 + 255) / 510;
    if maximum == minimum {
        return Hsl240 {
            hue: 160,
            saturation: 0,
            lightness: u16::try_from(lightness).expect("HSL lightness is at most 240"),
        };
    }

    let range = maximum - minimum;
    let denominator = if lightness > 120 { 510 - sum } else { sum };
    let saturation = (range / 2 + range * 240) / denominator;
    let half_range = range / 2;
    let red_distance = (half_range + (maximum - red) * 40) / range;
    let green_distance = (half_range + (maximum - green) * 40) / range;
    let blue_distance = (half_range + (maximum - blue) * 40) / range;
    let mut hue = if maximum == red {
        i32::from(blue_distance) - i32::from(green_distance)
    } else if maximum == green {
        i32::from(red_distance) - i32::from(blue_distance) + 80
    } else {
        i32::from(green_distance) - i32::from(red_distance) + 160
    };
    if hue < 0 {
        hue += 240;
    }
    if hue > 240 {
        hue -= 240;
    }
    Hsl240 {
        hue: u16::try_from(hue).expect("HSL hue remains in 0..=240"),
        saturation,
        lightness: u16::try_from(lightness).expect("HSL lightness is at most 240"),
    }
}

fn hsl_ordering_distance(previous: u16, candidate: u16) -> u32 {
    let previous = lunar_magic_hsl240(previous);
    let candidate = lunar_magic_hsl240(candidate);
    let hue = u32::from(circular_hue_distance(previous.hue, candidate.hue));
    let saturation = u32::from(previous.saturation.abs_diff(candidate.saturation));
    let lightness = u32::from(previous.lightness.abs_diff(candidate.lightness));
    if previous.saturation < 16 && candidate.saturation < 16 {
        lightness * lightness * 3 + saturation * saturation
    } else {
        lightness * lightness * 3 + (saturation * saturation + hue * hue * 4) * 2
    }
}

const fn circular_hue_distance(left: u16, right: u16) -> u16 {
    let direct = left.abs_diff(right);
    if direct > 120 { 240 - direct } else { direct }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BitmapPaletteReductionError {
    EntryCount(usize),
    MaximumColors(usize),
    PriorityLevel(u8),
    HueTolerance(u16),
    OriginalPaletteRequired,
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
        assert!(options.prioritize_unique_colors);
        assert!(!options.maintain_detail);
        assert!(options.allow_modifying_unmarked_colors);
        assert!(options.prioritize_exact_palette_matches);
        assert_eq!(options.reusable_color_hue_tolerance, 45);
        assert!(options.popularity_reduction_method_1);
        assert!(!options.popularity_reduction_method_2);
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
    fn median_cut_limit_cannot_exceed_installable_and_distinct_reusable_colors() {
        let mut options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.maximum_colors = 32;
        options.entries.fill(BitmapPaletteEntryState::Reserved);
        for entry in (0..BITMAP_PALETTE_COLORS)
            .filter(|entry| entry % Palette::COLORS_PER_ROW != 0)
            .take(21)
        {
            options.entries[entry] = BitmapPaletteEntryState::Free;
        }
        for row in 0..8 {
            options.entries[row * Palette::COLORS_PER_ROW] = BitmapPaletteEntryState::Reusable;
        }
        let mut palette = Palette {
            colors: vec![Bgr555(0); BITMAP_PALETTE_COLORS],
        };

        assert_eq!(native_reduction_color_limit(Some(&palette), &options), 22);
        assert_eq!(native_reduction_color_limit(None, &options), 32);

        for row in 0..8 {
            palette.colors[row * Palette::COLORS_PER_ROW] =
                Bgr555(u16::try_from(row).expect("the fixture row fits u16"));
        }
        assert_eq!(native_reduction_color_limit(Some(&palette), &options), 29);

        options.maximum_colors = 16;
        assert_eq!(native_reduction_color_limit(Some(&palette), &options), 16);
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
        assert_eq!(reduced.indices, [2, 2, 1, 0]);
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
        options.prioritize_unique_colors = false;
        let disabled = select_popularity_colors(&histogram, Some(&palette), &options).unwrap();

        assert_eq!(low, vec![Bgr555(1), Bgr555(0x7fff)]);
        assert_eq!(high, vec![Bgr555(0x03e0), Bgr555(1)]);
        assert_eq!(disabled, vec![Bgr555(1), Bgr555(0x7fff)]);
    }

    #[test]
    fn popularity_method_1_replaces_the_first_adjacent_weaker_color() {
        let mut selected = vec![(0x0421, 30), (0x0842, 20), (0x0c63, 10)];
        assert!(apply_popularity_reduction_method_1(
            &mut selected,
            0x0843,
            40
        ));
        assert_eq!(selected, vec![(0x0843, 40), (0x0421, 30), (0x0c63, 10)]);

        assert!(apply_popularity_reduction_method_1(
            &mut selected,
            0x0844,
            20
        ));
        assert_eq!(selected, vec![(0x0843, 40), (0x0421, 30), (0x0844, 20)]);
    }

    #[test]
    fn popularity_neighborhoods_preserve_native_empty_underflow_scans() {
        assert_eq!(component_range_start(0, 1), u16::MAX);
        assert_eq!(component_range_start(0, 2), u16::MAX - 1);
        assert_eq!(component_range_start(1, 2), u16::MAX);

        let mut method_1 = vec![(0x0001, 10)];
        assert!(!apply_popularity_reduction_method_1(
            &mut method_1,
            0x0000,
            20
        ));
        assert_eq!(method_1, vec![(0x0001, 10)]);

        let mut method_2 = vec![(0x0001, 10)];
        assert!(!apply_popularity_reduction_method_2(
            &mut method_2,
            0x0000,
            20
        ));
        assert_eq!(method_2, vec![(0x0001, 10)]);
    }

    #[test]
    fn popularity_method_1_stops_after_its_first_weaker_replacement() {
        let mut selected = vec![(0x0422, 30), (0x0424, 20), (0x7fff, 10)];
        assert!(apply_popularity_reduction_method_1(
            &mut selected,
            0x0423,
            40
        ));
        assert_eq!(selected, vec![(0x0423, 40), (0x0424, 20), (0x7fff, 10)]);
    }

    #[test]
    fn popularity_method_2_rejects_on_the_first_strong_neighbor() {
        // Red is the outer neighborhood loop, so $0421 is visited before $0422.
        let mut selected = vec![(0x0421, 100), (0x0422, 40), (0x7fff, 20)];
        assert!(apply_popularity_reduction_method_2(
            &mut selected,
            0x0423,
            50
        ));
        assert_eq!(selected, vec![(0x0421, 100), (0x0422, 40), (0x7fff, 20)]);
    }

    #[test]
    fn popularity_method_2_combines_only_a_nearby_sub_128_score() {
        let mut selected = vec![(0x7fff, 200), (0x7000, 60), (0x0421, 40)];
        assert!(apply_popularity_reduction_method_2(
            &mut selected,
            0x0843,
            50
        ));
        assert_eq!(selected, vec![(0x7fff, 200), (0x7000, 60), (0x0843, 90)]);

        selected[1] = (0x0842, 110);
        assert!(apply_popularity_reduction_method_2(
            &mut selected,
            0x0844,
            100
        ));
        assert_eq!(selected, vec![(0x7fff, 200), (0x0842, 110), (0x0843, 90)]);
    }

    #[test]
    fn ordinary_source_mapping_uses_native_weighted_rgb555_distance() {
        let source = Bgr555(0x008c).to_rgb8();
        let colors = [Bgr555(0x0180), Bgr555(0x3004)];

        // Unweighted expanded-RGB distance prefers $0180. Lunar Magic includes its zero sentinel,
        // then weights red, green, and blue by 4:3:2 in RGB555 space and selects $3004.
        assert_eq!(
            Palette {
                colors: colors.to_vec()
            }
            .quantize(&[source]),
            Some(vec![0])
        );
        assert_eq!(
            nearest_lunar_magic_palette_indices(&[source], &colors).unwrap(),
            vec![2]
        );
    }

    #[test]
    fn exact_allocator_assigns_the_seed_proposal_to_the_best_target_row() {
        let mut options = reserved_options();
        options.entries[1] = BitmapPaletteEntryState::Free;
        options.entries[2] = BitmapPaletteEntryState::Free;
        options.entries[17] = BitmapPaletteEntryState::Free;
        let mut rows =
            std::array::from_fn(|row| PaletteRowAllocation::new(row, &palette(), &options));
        let tile_sets = [
            TileColorHistogram {
                colors: vec![1],
                weights: vec![64],
            },
            TileColorHistogram {
                colors: vec![1, 2],
                weights: vec![32, 32],
            },
            TileColorHistogram {
                colors: vec![3],
                weights: vec![64],
            },
        ];
        let mut records = build_color_set_records(&tile_sets);

        assign_color_set_records(&mut records, &mut rows).unwrap();

        assert_eq!(rows[0].colors(), vec![1, 2]);
        assert_eq!(rows[1].colors(), vec![3]);
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
            red: 255,
            green: 0,
            blue: 0,
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

    #[test]
    fn low_color_bitmap_uses_lunar_magic_bit_two_rounding() {
        let source = [
            Rgba8 {
                red: 0x9c,
                green: 0xe7,
                blue: 0xe7,
                alpha: 255,
            },
            Rgba8 {
                red: 0xad,
                green: 0xe7,
                blue: 0xff,
                alpha: 255,
            },
            Rgba8 {
                red: 0xc6,
                green: 0xff,
                blue: 0xff,
                alpha: 255,
            },
            Rgba8 {
                red: 0xef,
                green: 0xf7,
                blue: 0xff,
                alpha: 255,
            },
        ];
        let reduced =
            reduce_bitmap_palette(&source, &BitmapPaletteColorOptions::lunar_magic_initial())
                .unwrap();
        assert_eq!(
            reduced.colors,
            [
                Bgr555(0x77b4),
                Bgr555(0x7fb6),
                Bgr555(0x7ff9),
                Bgr555(0x7ffe)
            ]
        );
        assert_eq!(reduced.indices, [1, 2, 3, 4]);
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
            prioritize_unique_colors: true,
            maintain_detail: false,
            allow_modifying_unmarked_colors: true,
            prioritize_exact_palette_matches: true,
            reusable_color_hue_tolerance: 45,
            popularity_reduction_method_1: true,
            popularity_reduction_method_2: false,
        }
    }

    #[test]
    fn recovered_hsl240_uses_windows_integer_scale() {
        assert_eq!(
            lunar_magic_hsl240(0x001f),
            Hsl240 {
                hue: 0,
                saturation: 240,
                lightness: 120,
            }
        );
        assert_eq!(
            lunar_magic_hsl240(0x03e0),
            Hsl240 {
                hue: 80,
                saturation: 240,
                lightness: 120,
            }
        );
        assert_eq!(
            lunar_magic_hsl240(0x7c00),
            Hsl240 {
                hue: 160,
                saturation: 240,
                lightness: 120,
            }
        );
        assert_eq!(
            lunar_magic_hsl240(0x4210),
            Hsl240 {
                hue: 160,
                saturation: 0,
                lightness: 124,
            }
        );
    }

    #[test]
    fn reusable_palette_substitution_obeys_recovered_hue_policy() {
        let mut options = reserved_options();
        options.entries[1] = BitmapPaletteEntryState::Free;
        options.entries[17] = BitmapPaletteEntryState::Reusable;
        let mut original = palette();
        original.colors[17] = Bgr555(0x001e);

        let mut close_red = [Bgr555(0x001f)];
        substitute_reusable_palette_colors(&mut close_red, &original, &options).unwrap();
        assert_eq!(close_red, [Bgr555(0x001e)]);

        let mut blue = [Bgr555(0x7c00)];
        substitute_reusable_palette_colors(&mut blue, &original, &options).unwrap();
        assert_eq!(blue, [Bgr555(0x7c00)]);

        options.reusable_color_hue_tolerance = 240;
        substitute_reusable_palette_colors(&mut blue, &original, &options).unwrap();
        assert_eq!(blue, [Bgr555(0x001e)]);
    }

    #[test]
    fn reusable_palette_substitution_accepts_neutral_colors_outside_hue_limit() {
        let mut options = reserved_options();
        options.entries[1] = BitmapPaletteEntryState::Free;
        options.entries[17] = BitmapPaletteEntryState::Reusable;
        options.reusable_color_hue_tolerance = 0;
        let mut original = palette();
        original.colors[17] = Bgr555(0x4210);
        let mut colors = [Bgr555(0x39ce)];

        substitute_reusable_palette_colors(&mut colors, &original, &options).unwrap();

        assert_eq!(colors, [Bgr555(0x4210)]);
    }

    #[test]
    fn reusable_palette_substitution_requires_a_free_destination_entry() {
        let mut options = reserved_options();
        options.entries[17] = BitmapPaletteEntryState::Reusable;
        let mut original = palette();
        original.colors[17] = Bgr555(0x001e);
        let mut colors = [Bgr555(0x001f)];

        substitute_reusable_palette_colors(&mut colors, &original, &options).unwrap();

        assert_eq!(colors, [Bgr555(0x001f)]);
    }

    #[test]
    fn reduction_retries_when_reusable_words_do_not_accept_generated_colors() {
        let mut options = reserved_options();
        options.maximum_colors = 2;
        options.entries[1] = BitmapPaletteEntryState::Free;
        options.entries[17] = BitmapPaletteEntryState::Reusable;
        let mut original = palette();
        original.colors[17] = Bgr555(0);
        let pixels = [
            Rgba8 {
                red: 255,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            Rgba8 {
                red: 0,
                green: 0,
                blue: 255,
                alpha: 255,
            },
        ];

        let reduced = reduce_bitmap_palette_with_palette(&pixels, &original, &options).unwrap();

        assert_eq!(native_reduction_color_limit(Some(&original), &options), 2);
        assert_eq!(reduced.colors.len(), 1);
        assert_ne!(reduced.colors, [Bgr555(0)]);
    }

    #[test]
    fn reusable_palette_substitution_excludes_each_transparency_entry() {
        let mut options = reserved_options();
        options.entries[1] = BitmapPaletteEntryState::Free;
        options.entries[16] = BitmapPaletteEntryState::Reusable;
        let mut original = palette();
        original.colors[16] = Bgr555(0x4210);
        let mut colors = [Bgr555(0x39ce)];

        substitute_reusable_palette_colors(&mut colors, &original, &options).unwrap();

        assert_eq!(colors, [Bgr555(0x39ce)]);
    }

    #[test]
    fn disabling_unmarked_color_modification_reuses_the_native_nearest_palette_word() {
        let mut options = reserved_options();
        options.allow_modifying_unmarked_colors = false;
        options.entries[1] = BitmapPaletteEntryState::Free;
        options.entries[17] = BitmapPaletteEntryState::Reusable;
        let mut original = palette();
        original.colors[1] = Bgr555(0x001f);
        original.colors[17] = Bgr555(0x7c00);
        let pixels = vec![
            Rgba8 {
                red: 0,
                green: 255,
                blue: 0,
                alpha: 255,
            };
            64
        ];

        let reduced = reduce_bitmap_palette_with_palette(&pixels, &original, &options).unwrap();
        assert_eq!(reduced.colors, [Bgr555(0x001f), Bgr555(0x7c00)]);
        let allocated = allocate_bitmap_palette_rows(&reduced, 8, 8, &original, &options).unwrap();

        assert!(
            reduced
                .colors
                .iter()
                .all(|color| [Bgr555(0x001f), Bgr555(0x7c00)].contains(color))
        );
        assert_eq!(reduced.indices, vec![0; 64]);
        assert_eq!(allocated.tile_rows, vec![0]);
        assert_eq!(allocated.generated_colors, 0);
        for (index, (before, after)) in original
            .colors
            .iter()
            .zip(&allocated.palette.colors)
            .enumerate()
        {
            if options.entries[index] == BitmapPaletteEntryState::Reserved {
                assert_eq!(after, before);
            } else if before != after {
                assert!([Bgr555(0x001f), Bgr555(0x7c00)].contains(after));
            }
        }
    }

    #[test]
    fn no_modification_mode_requires_destination_palette_context() {
        let mut options = reserved_options();
        options.allow_modifying_unmarked_colors = false;
        let error = reduce_bitmap_palette(
            &[Rgba8 {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            }],
            &options,
        )
        .unwrap_err();

        assert_eq!(error, BitmapPaletteReductionError::OriginalPaletteRequired);
    }

    #[test]
    fn available_palette_deduplication_uses_native_tail_replacement_order() {
        let mut options = reserved_options();
        let mut original = palette();
        for (index, color) in [1, 2, 1, 4, 5].into_iter().enumerate() {
            options.entries[index + 1] = BitmapPaletteEntryState::Free;
            original.colors[index + 1] = Bgr555(color);
        }

        assert_eq!(
            collect_unique_available_palette_colors(&original, &options).unwrap(),
            [Bgr555(1), Bgr555(2), Bgr555(5), Bgr555(4)]
        );
    }

    #[test]
    fn native_exact_match_preference_is_persisted_but_conversion_neutral() {
        let pixels = [Rgba8 {
            red: 248,
            green: 0,
            blue: 0,
            alpha: 255,
        }];
        let original = palette();
        let mut enabled = BitmapPaletteColorOptions::lunar_magic_initial();
        let mut disabled = enabled.clone();
        disabled.prioritize_exact_palette_matches = false;

        assert_eq!(
            reduce_bitmap_palette_with_palette(&pixels, &original, &enabled).unwrap(),
            reduce_bitmap_palette_with_palette(&pixels, &original, &disabled).unwrap()
        );
        enabled.prioritize_exact_palette_matches = false;
        assert_eq!(enabled, disabled);
    }

    #[test]
    fn generated_row_colors_begin_each_hue_run_at_lowest_lightness() {
        let mut row = PaletteRowAllocation {
            row: 0,
            entries: [
                RowEntry::Reserved,
                RowEntry::Assigned(0x7fff),
                RowEntry::Assigned(0x001f),
                RowEntry::Assigned(0x0000),
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
                RowEntry::Reserved,
            ],
            first_reusable_color: None,
        };
        row.order_assigned_colors(false);
        assert!(matches!(row.entries[1], RowEntry::Assigned(0x0000)));
    }

    #[test]
    fn unmarked_mode_rebuilds_a_later_row_from_colors_retained_by_an_earlier_row() {
        let red = Bgr555(0x001f);
        let blue = Bgr555(0x7c00);
        let mut original = palette();
        original.colors[1] = red;
        original.colors[2] = blue;
        original.colors[17] = Bgr555(0x03e0);
        original.colors[18] = Bgr555(0x7fff);
        let mut options = reserved_options();
        options.allow_modifying_unmarked_colors = false;
        for entry in [1, 2, 17, 18] {
            options.entries[entry] = BitmapPaletteEntryState::Free;
        }
        let reduced = ReducedBitmapPalette {
            colors: vec![red, blue],
            indices: (0..64)
                .map(|pixel| if pixel & 1 == 0 { 1 } else { 2 })
                .collect(),
        };

        let allocated = allocate_bitmap_palette_rows(&reduced, 8, 8, &original, &options).unwrap();

        assert_eq!(allocated.palette.colors[1..=2], [red, blue]);
        assert_eq!(allocated.palette.colors[17..=18], [red, blue]);
        assert_eq!(allocated.tile_rows, [1]);
        assert_eq!(&allocated.indices[..4], &[1, 2, 1, 2]);
    }

    #[test]
    fn unmarked_mode_uses_the_last_equal_palette_entry() {
        let color = 0x7393;
        let row = PaletteRowAllocation {
            row: 1,
            entries: std::array::from_fn(|entry| match entry {
                4 => RowEntry::Assigned(color),
                12 => RowEntry::Free(Some(color)),
                _ => RowEntry::Reserved,
            }),
            first_reusable_color: None,
        };

        assert_eq!(row.nearest(color, false), Some((4, 0)));
        assert_eq!(row.nearest(color, true), Some((12, 0)));
    }

    #[test]
    fn generated_row_hsl_distance_uses_native_lightness_and_saturation_weights() {
        assert_eq!(hsl_ordering_distance(26151, 25162), 1352);
    }

    #[test]
    fn allocator_preserves_row_zero_reusable_color_before_native_sentinel_tie() {
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
        // AssignImportedGraphicsToPaletteRows uses row zero as its no-result sentinel. An exact
        // row-zero/row-one tie therefore resolves to row one even though row zero retained red.
        assert_eq!(allocated.tile_rows, [1]);
        assert_eq!(allocated.palette.colors[1], red);
        assert_eq!(allocated.palette.colors[2], Bgr555(0));
        assert_eq!(allocated.palette.colors[17..=18], [red, blue]);
        assert_eq!(allocated.generated_colors, 2);
        assert_eq!(&allocated.indices[..4], &[1, 2, 1, 2]);
    }

    #[test]
    fn maintain_detail_skips_weighted_partial_set_extension() {
        let colors = [
            Bgr555(0x001f),
            Bgr555(0x03e0),
            Bgr555(0x7c00),
            Bgr555(0x7fff),
        ];
        let mut original = palette();
        original.colors[1] = Bgr555(0x001f);
        let mut options = reserved_options();
        options.entries[1] = BitmapPaletteEntryState::Reusable;
        options.entries[2] = BitmapPaletteEntryState::Free;
        options.entries[3] = BitmapPaletteEntryState::Free;
        let record = ColorSetRecord {
            colors: colors.iter().map(|color| color.0).collect(),
            tiles: vec![0],
            direct_weights: vec![40, 12, 8, 4],
            aggregate_weights: vec![40, 12, 8, 4],
            aggregate_weight: 64,
            assigned_row: None,
        };
        let key = record.colors.clone();
        let mut records = BTreeMap::from([(key, record)]);
        let mut rows =
            std::array::from_fn(|row| PaletteRowAllocation::new(row, &original, &options));

        assign_color_set_records(&mut records, &mut rows).unwrap();
        assert_eq!(rows[0].colors(), [Bgr555(0x001f).0]);
        extend_palette_rows_with_weighted_colors(&mut records, &mut rows, false).unwrap();
        assert_eq!(rows[0].colors().len(), 3);
    }

    #[test]
    fn maintain_detail_claims_one_distinct_source_color_per_palette_color() {
        let opaque = [Bgr555(0).to_rgb8(), Bgr555(2).to_rgb8()];
        let colors = [Bgr555(0), Bgr555(0x001f)];
        let detailed = maintain_detail_palette_indices(&opaque, &colors, &[]).unwrap();
        let nearest = Palette {
            colors: colors.to_vec(),
        }
        .quantize(&opaque)
        .unwrap();

        assert_eq!(nearest, [0, 0]);
        assert_eq!(detailed, [0, 1]);
    }

    #[test]
    fn maintain_detail_zero_sentinel_claims_the_nearest_unused_source_color() {
        let colors = [Bgr555(0x1000), Bgr555(0x2000)];
        let opaque = [
            colors[0].to_rgb8(),
            colors[1].to_rgb8(),
            Bgr555(1).to_rgb8(),
        ];

        assert_eq!(
            maintain_detail_palette_indices(&opaque, &colors, &[]).unwrap(),
            [1, 2, 0]
        );
    }

    #[test]
    fn maintain_detail_exact_existing_black_bypasses_the_zero_sentinel() {
        let colors = [Bgr555(0x0010), Bgr555(0)];
        let opaque = [Bgr555(0).to_rgb8(), Bgr555(0x001f).to_rgb8()];
        assert_eq!(
            maintain_detail_palette_indices(&opaque, &colors, &[0]).unwrap(),
            [2, 1]
        );
    }

    #[test]
    fn maintain_detail_materializes_the_opaque_zero_candidate() {
        let near_black = Bgr555(1);
        let red = Bgr555(0x001f);
        let mut original = Palette {
            colors: vec![Bgr555(0x7fff); BITMAP_PALETTE_COLORS],
        };
        original.colors[13] = Bgr555(0);
        let mut options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.maximum_colors = 1;
        options.maintain_detail = true;
        options.entries.fill(BitmapPaletteEntryState::Reserved);
        options.entries[0] = BitmapPaletteEntryState::Reusable;
        options.entries[13] = BitmapPaletteEntryState::Free;
        options.entries[16] = BitmapPaletteEntryState::Reusable;
        options.entries[17] = BitmapPaletteEntryState::Free;
        let pixels = [near_black.to_rgb8(), red.to_rgb8()].map(|pixel| Rgba8 {
            red: pixel.red,
            green: pixel.green,
            blue: pixel.blue,
            alpha: 255,
        });

        let reduced = reduce_bitmap_palette_with_palette(&pixels, &original, &options).unwrap();
        assert!(reduced.colors.contains(&Bgr555(0)));
        assert!(reduced.indices.iter().all(|index| *index != 0));
    }

    #[test]
    fn ordinary_reduction_prefers_an_exact_color_but_retains_the_zero_fallback() {
        let opaque = [Bgr555(0).to_rgb8(), Bgr555(0x7fff).to_rgb8()];
        assert_eq!(
            nearest_lunar_magic_palette_indices(&opaque, &[Bgr555(0x7fff)]).unwrap(),
            [0, 1]
        );
        assert_eq!(
            nearest_lunar_magic_palette_indices(&opaque[..1], &[Bgr555(0)]).unwrap(),
            [1]
        );
    }

    #[test]
    fn exact_usable_black_bypasses_the_generated_color_limit() {
        let black = Bgr555(0);
        let red = Bgr555(0x001f);
        let mut original = Palette {
            colors: vec![Bgr555(0x7fff); BITMAP_PALETTE_COLORS],
        };
        original.colors[13] = black;
        let mut options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.maximum_colors = 1;
        options.entries.fill(BitmapPaletteEntryState::Reserved);
        options.entries[0] = BitmapPaletteEntryState::Reusable;
        options.entries[13] = BitmapPaletteEntryState::Free;
        options.entries[16] = BitmapPaletteEntryState::Reusable;
        options.entries[17] = BitmapPaletteEntryState::Free;
        let pixels = [black.to_rgb8(), red.to_rgb8()].map(|pixel| Rgba8 {
            red: pixel.red,
            green: pixel.green,
            blue: pixel.blue,
            alpha: 255,
        });

        let reduced = reduce_bitmap_palette_with_palette(&pixels, &original, &options).unwrap();
        assert_eq!(reduced.colors.len(), 2);
        assert!(reduced.colors.contains(&black));
        assert_ne!(reduced.indices[0], 0);
        assert_ne!(reduced.indices[1], 0);
    }

    #[test]
    fn exact_free_row_color_is_used_without_installing_a_duplicate() {
        let black = Bgr555(0);
        let generated = Bgr555(0x0010);
        let mut original = Palette {
            colors: vec![Bgr555(0x7fff); BITMAP_PALETTE_COLORS],
        };
        original.colors[13] = black;
        let mut options = BitmapPaletteColorOptions::lunar_magic_initial();
        options.entries.fill(BitmapPaletteEntryState::Reserved);
        options.entries[0] = BitmapPaletteEntryState::Reusable;
        options.entries[13] = BitmapPaletteEntryState::Free;
        options.entries[16] = BitmapPaletteEntryState::Reusable;
        options.entries[17] = BitmapPaletteEntryState::Free;
        let reduced = ReducedBitmapPalette {
            colors: vec![generated, black],
            indices: (0..128)
                .map(|pixel| if pixel % 16 < 8 { 2 } else { 1 })
                .collect(),
        };

        let allocated = allocate_bitmap_palette_rows(&reduced, 16, 8, &original, &options).unwrap();
        assert_eq!(allocated.generated_colors, 1);
        assert_eq!(allocated.palette.colors[13], black);
        assert_eq!(allocated.palette.colors[17], generated);
        assert_eq!(allocated.tile_rows, [0, 1]);
        assert!(allocated.indices.chunks_exact(16).all(|row| {
            row[..8].iter().all(|index| *index == 13) && row[8..].iter().all(|index| *index == 1)
        }));
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
    fn high_color_tile_is_reduced_to_the_native_row_capacity() {
        let colors = (0_u16..16).map(Bgr555).collect::<Vec<_>>();
        let reduced = ReducedBitmapPalette {
            colors,
            indices: (0..64)
                .map(|index| u8::try_from(index % 16 + 1).unwrap())
                .collect(),
        };
        let allocated = allocate_bitmap_palette_rows(
            &reduced,
            8,
            8,
            &palette(),
            &BitmapPaletteColorOptions::lunar_magic_initial(),
        )
        .unwrap();
        assert!(allocated.generated_colors <= 8);
    }

    #[test]
    fn exact_capacity_tile_without_the_reusable_first_color_drops_its_last_weak_tie() {
        let words = [
            0x3696, 0x4e8c, 0x5291, 0x5313, 0x6180, 0x61c2, 0x6205, 0x6648, 0x666a, 0x6aad, 0x6ad0,
            0x6f33,
        ];
        let weights = [1, 1, 1, 1, 3, 2, 10, 11, 7, 12, 9, 6];
        let mut indices = Vec::new();
        for (index, weight) in weights.into_iter().enumerate() {
            indices.extend(std::iter::repeat_n(
                u8::try_from(index + 1).unwrap(),
                weight,
            ));
        }
        assert_eq!(indices.len(), 64);
        let mut reduced = ReducedBitmapPalette {
            colors: words.into_iter().map(Bgr555).collect(),
            indices,
        };
        let mut options = reserved_options();
        options.entries[0] = BitmapPaletteEntryState::Reusable;
        for entry in 1..=12 {
            options.entries[entry] = BitmapPaletteEntryState::Free;
        }
        let original = palette();
        let rows = std::array::from_fn(|row| PaletteRowAllocation::new(row, &original, &options));

        reduce_tiles_to_native_row_capacity(&mut reduced, 8, 8, 1, 1, &rows).unwrap();
        let histogram = tile_color_histogram(&reduced, 8, 0, 0).unwrap();

        assert_eq!(histogram.colors.len(), 11);
        assert!(!histogram.colors.contains(&0x5313));
        assert_eq!(
            histogram.weights[histogram.colors.binary_search(&0x5291).unwrap()],
            2
        );
    }
}
