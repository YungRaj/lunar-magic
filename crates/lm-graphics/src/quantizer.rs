use crate::{Bgr555, Palette, Rgb8};
use std::fmt;

mod wu;

use wu::{ColorBox, best_split, build_moments, variance, volume};

const MAX_COLORS: usize = 256;
const MAX_PIXELS: usize = 16 * 1024 * 1024;

/// Deterministic bitmap quantization result in the editor's native SNES color space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuantizedImage {
    pub palette: Palette,
    pub indices: Vec<u8>,
}

/// Variance-minimizing RGB quantizer recovered from Lunar Magic's bitmap import boundary.
pub struct WuQuantizer;

impl WuQuantizer {
    /// Builds at most `maximum_colors` representative SNES BGR555 colors and maps every pixel.
    ///
    /// Histogram bins use the 5-bit SNES channel lattice. Boxes are split by the weighted RGB
    /// variance reduction described by Wu's quantizer, with stable red/green/blue and low-cut tie
    /// ordering so output is byte-identical across platforms.
    ///
    /// # Errors
    ///
    /// Returns [`QuantizerError`] for a zero/excessive palette bound, more than 16 Mi pixels, or
    /// an internal result that cannot be represented by one-byte palette indexes.
    pub fn quantize(
        pixels: &[Rgb8],
        maximum_colors: usize,
    ) -> Result<QuantizedImage, QuantizerError> {
        if maximum_colors == 0 || maximum_colors > MAX_COLORS {
            return Err(QuantizerError::InvalidColorCount(maximum_colors));
        }
        if pixels.len() > MAX_PIXELS {
            return Err(QuantizerError::TooManyPixels(pixels.len()));
        }
        if pixels.is_empty() {
            return Ok(QuantizedImage {
                palette: Palette { colors: Vec::new() },
                indices: Vec::new(),
            });
        }

        let moments = build_moments(pixels);
        let mut boxes = vec![ColorBox::whole()];
        while boxes.len() < maximum_colors {
            let candidate = boxes
                .iter()
                .enumerate()
                .filter_map(|(index, color_box)| {
                    best_split(&moments, *color_box)
                        .map(|split| (index, variance(&moments, *color_box), split))
                })
                .max_by(|left, right| {
                    left.1
                        .total_cmp(&right.1)
                        .then_with(|| right.0.cmp(&left.0))
                });
            let Some((index, _, split)) = candidate else {
                break;
            };
            boxes[index] = split.first;
            boxes.push(split.second);
        }

        let mut colors = Vec::with_capacity(boxes.len());
        for color_box in boxes {
            let moment = volume(&moments, color_box);
            if moment.weight > 0.0 {
                colors.push(Bgr555::from_rgb8(Rgb8 {
                    red: rounded_mean(moment.red, moment.weight),
                    green: rounded_mean(moment.green, moment.weight),
                    blue: rounded_mean(moment.blue, moment.weight),
                }));
            }
        }
        let mut unique = Vec::with_capacity(colors.len());
        for color in colors {
            if !unique.contains(&color) {
                unique.push(color);
            }
        }
        let colors = unique;
        let palette = Palette { colors };
        let indices = palette
            .quantize(pixels)
            .ok_or(QuantizerError::UnrepresentableResult)?
            .into_iter()
            .map(|index| u8::try_from(index).map_err(|_| QuantizerError::UnrepresentableResult))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(QuantizedImage { palette, indices })
    }
}

fn rounded_mean(sum: f64, weight: f64) -> u8 {
    let rounded = (sum / weight).round().clamp(0.0, 255.0);
    (0..=u8::MAX)
        .find(|value| f64::from(*value) >= rounded)
        .unwrap_or(u8::MAX)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QuantizerError {
    InvalidColorCount(usize),
    TooManyPixels(usize),
    UnrepresentableResult,
}

impl fmt::Display for QuantizerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bitmap quantization failed: {self:?}")
    }
}

impl std::error::Error for QuantizerError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separated_primary_clusters_get_stable_snes_colors() {
        let pixels = [
            Rgb8 {
                red: 255,
                green: 0,
                blue: 0,
            },
            Rgb8 {
                red: 248,
                green: 8,
                blue: 0,
            },
            Rgb8 {
                red: 0,
                green: 255,
                blue: 0,
            },
            Rgb8 {
                red: 8,
                green: 248,
                blue: 0,
            },
            Rgb8 {
                red: 0,
                green: 0,
                blue: 255,
            },
            Rgb8 {
                red: 0,
                green: 8,
                blue: 248,
            },
        ];
        let result = WuQuantizer::quantize(&pixels, 3).unwrap();
        assert_eq!(result.palette.colors.len(), 3);
        assert_eq!(result.indices[0], result.indices[1]);
        assert_eq!(result.indices[2], result.indices[3]);
        assert_eq!(result.indices[4], result.indices[5]);
        assert_ne!(result.indices[0], result.indices[2]);
        assert_ne!(result.indices[2], result.indices[4]);
        assert_eq!(WuQuantizer::quantize(&pixels, 3).unwrap(), result);
    }

    #[test]
    fn uniform_and_empty_inputs_do_not_invent_colors() {
        let color = Rgb8 {
            red: 17,
            green: 99,
            blue: 201,
        };
        let uniform = WuQuantizer::quantize(&[color; 64], 16).unwrap();
        assert_eq!(uniform.palette.colors, [Bgr555::from_rgb8(color)]);
        assert_eq!(uniform.indices, vec![0; 64]);
        assert_eq!(
            WuQuantizer::quantize(&[], 4).unwrap(),
            QuantizedImage {
                palette: Palette { colors: Vec::new() },
                indices: Vec::new(),
            }
        );
    }

    #[test]
    fn color_and_pixel_limits_are_explicit() {
        assert_eq!(
            WuQuantizer::quantize(&[Rgb8::default()], 0),
            Err(QuantizerError::InvalidColorCount(0))
        );
        assert_eq!(
            WuQuantizer::quantize(&[Rgb8::default()], 257),
            Err(QuantizerError::InvalidColorCount(257))
        );
        assert!(matches!(
            WuQuantizer::quantize(&vec![Rgb8::default(); MAX_PIXELS + 1], 1),
            Err(QuantizerError::TooManyPixels(_))
        ));
    }
}
