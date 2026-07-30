//! Lunar Magic-compatible color-option primitives for bitmap graphics imports.

use crate::{Bgr555, Palette, QuantizerError, Rgb8, Rgba8, WuQuantizer};
use std::{collections::BTreeMap, fmt};

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

/// Applies the selected global 1–128-color reduction before palette-row assignment.
///
/// Transparent pixels receive index zero in `indices` and do not consume a reduced color. Opaque
/// indexes are stored one-based so zero remains unambiguous. When the source already fits the
/// bound, colors are ordered by RGB555 value. Popularity orders by descending frequency then
/// RGB555 value; median-cut delegates to the deterministic variance-splitting quantizer.
///
/// # Errors
///
/// Returns [`BitmapPaletteReductionError`] for invalid options, fractional alpha, excessive
/// quantizer input, or an unrepresentable one-based color index.
pub fn reduce_bitmap_palette(
    pixels: &[Rgba8],
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
                let mut weighted = histogram.into_iter().collect::<Vec<_>>();
                weighted.sort_by(|(left_color, left_count), (right_color, right_count)| {
                    right_count
                        .cmp(left_count)
                        .then_with(|| left_color.cmp(right_color))
                });
                weighted
                    .into_iter()
                    .take(options.maximum_colors)
                    .map(|(color, _)| Bgr555(color))
                    .collect()
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
    fn popularity_uses_frequency_then_rgb555_for_stable_ties() {
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
        assert_eq!(reduced.colors[0], Bgr555::from_rgb8(rgb(red)));
        assert_eq!(reduced.colors[1], Bgr555::from_rgb8(rgb(green)));
        assert!(reduced.indices.iter().all(|index| (1..=2).contains(index)));
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
}
