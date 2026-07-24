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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteImportError {
    RowOutOfRange(usize),
    Quantizer(QuantizerError),
    Palette(PaletteBatchEditError),
    IndexOutOfRange(u8),
    FractionalAlpha { index: usize, alpha: u8 },
    IndexPlaneMismatch,
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
