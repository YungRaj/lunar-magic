use crate::{Palette, PaletteBatchEditError, PaletteOwnership, QuantizerError, Rgb8, WuQuantizer};
use std::fmt;

/// A staged opaque bitmap palette row and its transparency-safe 4bpp indexes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaquePaletteRowImport {
    pub palette: Palette,
    pub indices: Vec<u8>,
    pub generated_colors: usize,
}

/// One unassociated-alpha RGB pixel for importing artwork with SNES-style transparency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgba8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

/// A staged palette row and 4bpp indexes produced from binary-alpha artwork.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransparentPaletteRowImport {
    pub palette: Palette,
    pub indices: Vec<u8>,
    pub generated_colors: usize,
}

impl TransparentPaletteRowImport {
    /// Maps fully transparent pixels to index zero and quantizes opaque pixels into entries 1–15.
    ///
    /// SNES 4bpp tile data has no fractional alpha, so values other than zero and 255 are rejected
    /// instead of being silently rounded. Palette ownership and row bounds are validated even when
    /// every source pixel is transparent.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteImportError`] for fractional alpha or any error from the underlying
    /// ownership-aware opaque-row import.
    pub fn quantize(
        pixels: &[Rgba8],
        row: usize,
        palette: &Palette,
        ownership: &PaletteOwnership,
    ) -> Result<Self, PaletteImportError> {
        let mut opaque = Vec::with_capacity(pixels.len());
        for (index, pixel) in pixels.iter().enumerate() {
            match pixel.alpha {
                0 => {}
                255 => opaque.push(Rgb8 {
                    red: pixel.red,
                    green: pixel.green,
                    blue: pixel.blue,
                }),
                alpha => return Err(PaletteImportError::FractionalAlpha { index, alpha }),
            }
        }
        let imported = OpaquePaletteRowImport::quantize(&opaque, row, palette, ownership)?;
        let mut opaque_indices = imported.indices.into_iter();
        let indices = pixels
            .iter()
            .map(|pixel| {
                if pixel.alpha == 0 {
                    Ok(0)
                } else {
                    opaque_indices
                        .next()
                        .ok_or(PaletteImportError::IndexPlaneMismatch)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if opaque_indices.next().is_some() {
            return Err(PaletteImportError::IndexPlaneMismatch);
        }
        Ok(Self {
            palette: imported.palette,
            indices,
            generated_colors: imported.generated_colors,
        })
    }

    /// Maps opaque pixels through preserved row colors and quantizes only into editable entries.
    ///
    /// This models bitmap-import color dialogs that let users reserve existing SNES colors. Entry
    /// zero remains transparency-only. Fixed or animation-owned entries 1–15 remain byte-exact
    /// and participate as color candidates; newly generated colors occupy editable entries in
    /// ascending order.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteImportError`] for fractional alpha, an invalid row/ownership shape, no
    /// usable opaque color entries, or quantization failure.
    pub fn quantize_preserving_owned(
        pixels: &[Rgba8],
        row: usize,
        palette: &Palette,
        ownership: &PaletteOwnership,
    ) -> Result<Self, PaletteImportError> {
        let mut opaque = Vec::with_capacity(pixels.len());
        for (index, pixel) in pixels.iter().enumerate() {
            match pixel.alpha {
                0 => {}
                255 => opaque.push(Rgb8 {
                    red: pixel.red,
                    green: pixel.green,
                    blue: pixel.blue,
                }),
                alpha => return Err(PaletteImportError::FractionalAlpha { index, alpha }),
            }
        }
        let imported =
            OpaquePaletteRowImport::quantize_preserving_owned(&opaque, row, palette, ownership)?;
        let mut opaque_indices = imported.indices.into_iter();
        let indices = pixels
            .iter()
            .map(|pixel| {
                if pixel.alpha == 0 {
                    Ok(0)
                } else {
                    opaque_indices
                        .next()
                        .ok_or(PaletteImportError::IndexPlaneMismatch)
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        if opaque_indices.next().is_some() {
            return Err(PaletteImportError::IndexPlaneMismatch);
        }
        Ok(Self {
            palette: imported.palette,
            indices,
            generated_colors: imported.generated_colors,
        })
    }
}

impl OpaquePaletteRowImport {
    /// Quantizes opaque RGB pixels into entries 1–15 of one SNES palette row.
    ///
    /// Entry zero is retained because renderers and SNES tile modes treat pixel index zero as
    /// transparent. Only generated destination entries are changed; unused colors elsewhere in
    /// the row and palette remain byte-exact. Ownership for every changed entry is validated before
    /// the staged palette is returned.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteImportError`] for an absent row, quantization failure, protected generated
    /// destination, ownership-shape mismatch, or an impossible 4bpp index conversion.
    pub fn quantize(
        pixels: &[Rgb8],
        row: usize,
        palette: &Palette,
        ownership: &PaletteOwnership,
    ) -> Result<Self, PaletteImportError> {
        let start = row
            .checked_mul(Palette::COLORS_PER_ROW)
            .and_then(|start| start.checked_add(1))
            .ok_or(PaletteImportError::RowOutOfRange(row))?;
        let row_end = start
            .checked_add(Palette::COLORS_PER_ROW - 1)
            .ok_or(PaletteImportError::RowOutOfRange(row))?;
        if row_end > palette.colors.len() {
            return Err(PaletteImportError::RowOutOfRange(row));
        }
        // Validate the complete ownership shape even for an empty bitmap.
        let mut staged = palette.clone();
        staged
            .apply_changes(&[], ownership)
            .map_err(PaletteImportError::Palette)?;
        if pixels.is_empty() {
            return Ok(Self {
                palette: staged,
                indices: Vec::new(),
                generated_colors: 0,
            });
        }
        let quantized = WuQuantizer::quantize(pixels, Palette::COLORS_PER_ROW - 1)
            .map_err(PaletteImportError::Quantizer)?;
        staged
            .replace_range(start, &quantized.palette.colors, ownership)
            .map_err(PaletteImportError::Palette)?;
        let indices = quantized
            .indices
            .into_iter()
            .map(|index| {
                index
                    .checked_add(1)
                    .filter(|index| *index < 16)
                    .ok_or(PaletteImportError::IndexOutOfRange(index))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            palette: staged,
            indices,
            generated_colors: quantized.palette.colors.len(),
        })
    }

    /// Preserves non-editable row colors, generates at most one color per editable entry, and
    /// maps every source pixel to the nearest combined preserved/generated candidate.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteImportError`] for invalid row or ownership shapes, a row with no usable
    /// opaque entry, quantizer failure, or an impossible 4bpp index.
    pub fn quantize_preserving_owned(
        pixels: &[Rgb8],
        row: usize,
        palette: &Palette,
        ownership: &PaletteOwnership,
    ) -> Result<Self, PaletteImportError> {
        let row_start = row
            .checked_mul(Palette::COLORS_PER_ROW)
            .ok_or(PaletteImportError::RowOutOfRange(row))?;
        let row_end = row_start
            .checked_add(Palette::COLORS_PER_ROW)
            .ok_or(PaletteImportError::RowOutOfRange(row))?;
        if row_end > palette.colors.len() {
            return Err(PaletteImportError::RowOutOfRange(row));
        }
        let mut staged = palette.clone();
        staged
            .apply_changes(&[], ownership)
            .map_err(PaletteImportError::Palette)?;
        if pixels.is_empty() {
            return Ok(Self {
                palette: staged,
                indices: Vec::new(),
                generated_colors: 0,
            });
        }
        let usable = (row_start + 1)..row_end;
        let editable = usable
            .clone()
            .filter(|index| ownership.owner(*index) == Some(crate::PaletteEntryOwner::Editable))
            .collect::<Vec<_>>();
        let preserved = usable
            .filter(|index| ownership.owner(*index) != Some(crate::PaletteEntryOwner::Editable))
            .collect::<Vec<_>>();
        let unmatched = pixels
            .iter()
            .copied()
            .filter(|pixel| {
                let color = crate::Bgr555::from_rgb8(*pixel);
                preserved.iter().all(|index| staged.colors[*index] != color)
            })
            .collect::<Vec<_>>();
        let generated = if editable.is_empty() || unmatched.is_empty() {
            Vec::new()
        } else {
            WuQuantizer::quantize(&unmatched, editable.len())
                .map_err(PaletteImportError::Quantizer)?
                .palette
                .colors
        };
        let changes = editable
            .iter()
            .copied()
            .zip(generated.iter().copied())
            .map(|(index, color)| crate::PaletteChange { index, color })
            .collect::<Vec<_>>();
        staged
            .apply_changes(&changes, ownership)
            .map_err(PaletteImportError::Palette)?;
        let candidates = preserved
            .into_iter()
            .chain(editable.into_iter().take(generated.len()))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(PaletteImportError::NoOpaqueColorEntries(row));
        }
        let indices = pixels
            .iter()
            .map(|pixel| {
                candidates
                    .iter()
                    .copied()
                    .min_by_key(|index| rgb_distance(staged.colors[*index].to_rgb8(), *pixel))
                    .and_then(|index| u8::try_from(index - row_start).ok())
                    .filter(|index| *index < 16)
                    .ok_or(PaletteImportError::IndexOutOfRange(u8::MAX))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            palette: staged,
            indices,
            generated_colors: generated.len(),
        })
    }
}

fn rgb_distance(left: Rgb8, right: Rgb8) -> i32 {
    let red = i32::from(left.red) - i32::from(right.red);
    let green = i32::from(left.green) - i32::from(right.green);
    let blue = i32::from(left.blue) - i32::from(right.blue);
    red * red + green * green + blue * blue
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteImportError {
    RowOutOfRange(usize),
    Quantizer(QuantizerError),
    Palette(PaletteBatchEditError),
    IndexOutOfRange(u8),
    FractionalAlpha { index: usize, alpha: u8 },
    IndexPlaneMismatch,
    NoOpaqueColorEntries(usize),
}

impl fmt::Display for PaletteImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "opaque bitmap palette import failed: {self:?}")
    }
}

impl std::error::Error for PaletteImportError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bgr555, PaletteEntryOwner};

    fn palette() -> Palette {
        Palette {
            colors: (0_u16..32).map(Bgr555).collect(),
        }
    }

    #[test]
    fn opaque_indexes_reserve_zero_and_preserve_unused_words() {
        let original = palette();
        let pixels = [
            Rgb8 {
                red: 255,
                green: 0,
                blue: 0,
            },
            Rgb8 {
                red: 0,
                green: 255,
                blue: 0,
            },
        ];
        let imported = OpaquePaletteRowImport::quantize(
            &pixels,
            1,
            &original,
            &PaletteOwnership::editable(32),
        )
        .unwrap();
        assert_eq!(imported.generated_colors, 2);
        assert!(
            imported
                .indices
                .iter()
                .all(|index| (1..=15).contains(index))
        );
        assert_eq!(imported.palette.colors[16], original.colors[16]);
        assert_eq!(&imported.palette.colors[19..], &original.colors[19..]);
    }

    #[test]
    fn protected_late_color_and_bad_rows_preserve_input() {
        let original = palette();
        let pixels = [
            Rgb8 {
                red: 255,
                green: 0,
                blue: 0,
            },
            Rgb8 {
                red: 0,
                green: 255,
                blue: 0,
            },
        ];
        let mut owners = PaletteOwnership::editable(32);
        owners.set_owner(18, PaletteEntryOwner::Fixed).unwrap();
        assert!(matches!(
            OpaquePaletteRowImport::quantize(&pixels, 1, &original, &owners),
            Err(PaletteImportError::Palette(
                PaletteBatchEditError::ProtectedColor { index: 18, .. }
            ))
        ));
        assert_eq!(original, palette());
        assert_eq!(
            OpaquePaletteRowImport::quantize(
                &pixels,
                2,
                &original,
                &PaletteOwnership::editable(32)
            ),
            Err(PaletteImportError::RowOutOfRange(2))
        );
    }

    #[test]
    fn ownership_aware_quantization_preserves_fixed_colors_as_candidates() {
        let mut original = palette();
        original.colors[17] = Bgr555::from_rgb8(Rgb8 {
            red: 255,
            green: 0,
            blue: 0,
        });
        let mut owners = PaletteOwnership::editable(32);
        owners.set_owner(17, PaletteEntryOwner::Fixed).unwrap();
        let pixels = [
            Rgb8 {
                red: 255,
                green: 0,
                blue: 0,
            },
            Rgb8 {
                red: 0,
                green: 0,
                blue: 255,
            },
        ];
        let imported =
            OpaquePaletteRowImport::quantize_preserving_owned(&pixels, 1, &original, &owners)
                .unwrap();
        assert_eq!(imported.palette.colors[17], original.colors[17]);
        assert_eq!(imported.indices[0], 1);
        assert_eq!(imported.generated_colors, 1);
    }

    #[test]
    fn fully_reserved_row_maps_without_mutating_palette() {
        let original = palette();
        let mut owners = PaletteOwnership::editable(32);
        for index in 17..32 {
            owners.set_owner(index, PaletteEntryOwner::Fixed).unwrap();
        }
        let imported = OpaquePaletteRowImport::quantize_preserving_owned(
            &[Rgb8 {
                red: 25,
                green: 50,
                blue: 75,
            }],
            1,
            &original,
            &owners,
        )
        .unwrap();
        assert_eq!(imported.palette, original);
        assert_eq!(imported.generated_colors, 0);
        assert!((1..=15).contains(&imported.indices[0]));
    }

    #[test]
    fn empty_input_validates_shape_without_changing_palette() {
        let original = palette();
        assert!(matches!(
            OpaquePaletteRowImport::quantize(&[], 0, &original, &PaletteOwnership::editable(31)),
            Err(PaletteImportError::Palette(
                PaletteBatchEditError::OwnershipShape { .. }
            ))
        ));
        let imported =
            OpaquePaletteRowImport::quantize(&[], 0, &original, &PaletteOwnership::editable(32))
                .unwrap();
        assert_eq!(imported.palette, original);
        assert!(imported.indices.is_empty());
    }

    #[test]
    fn binary_alpha_maps_transparency_to_zero_and_opaque_pixels_above_zero() {
        let original = palette();
        let pixels = [
            Rgba8 {
                red: 200,
                green: 20,
                blue: 10,
                alpha: 255,
            },
            Rgba8 {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 0,
            },
            Rgba8 {
                red: 10,
                green: 200,
                blue: 20,
                alpha: 255,
            },
        ];
        let imported = TransparentPaletteRowImport::quantize(
            &pixels,
            0,
            &original,
            &PaletteOwnership::editable(32),
        )
        .unwrap();
        assert_ne!(imported.indices[0], 0);
        assert_eq!(imported.indices[1], 0);
        assert_ne!(imported.indices[2], 0);
        assert_eq!(imported.palette.colors[0], original.colors[0]);
        assert_eq!(imported.generated_colors, 2);
    }

    #[test]
    fn all_transparent_input_preserves_palette_but_still_validates_ownership() {
        let original = palette();
        let pixels = vec![
            Rgba8 {
                red: 255,
                green: 0,
                blue: 255,
                alpha: 0,
            };
            64
        ];
        let imported = TransparentPaletteRowImport::quantize(
            &pixels,
            1,
            &original,
            &PaletteOwnership::editable(32),
        )
        .unwrap();
        assert_eq!(imported.palette, original);
        assert_eq!(imported.indices, vec![0; 64]);
        assert_eq!(imported.generated_colors, 0);
        assert!(matches!(
            TransparentPaletteRowImport::quantize(
                &pixels,
                1,
                &original,
                &PaletteOwnership::editable(31)
            ),
            Err(PaletteImportError::Palette(
                PaletteBatchEditError::OwnershipShape { .. }
            ))
        ));
    }

    #[test]
    fn fractional_alpha_is_rejected_before_palette_mutation() {
        let original = palette();
        let pixels = [Rgba8 {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 127,
        }];
        assert_eq!(
            TransparentPaletteRowImport::quantize(
                &pixels,
                0,
                &original,
                &PaletteOwnership::editable(32)
            ),
            Err(PaletteImportError::FractionalAlpha {
                index: 0,
                alpha: 127
            })
        );
        assert_eq!(original, palette());
    }
}
