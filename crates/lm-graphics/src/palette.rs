#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rgb8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Bgr555(pub u16);

impl Bgr555 {
    #[must_use]
    pub const fn from_rgb8(color: Rgb8) -> Self {
        let red = (color.red as u16 * 31 + 127) / 255;
        let green = (color.green as u16 * 31 + 127) / 255;
        let blue = (color.blue as u16 * 31 + 127) / 255;
        Self(red | (green << 5) | (blue << 10))
    }

    #[must_use]
    pub const fn to_rgb8(self) -> Rgb8 {
        let red = self.0 & 31;
        let green = (self.0 >> 5) & 31;
        let blue = (self.0 >> 10) & 31;
        Rgb8 {
            red: ((red * 255 + 15) / 31).to_le_bytes()[0],
            green: ((green * 255 + 15) / 31).to_le_bytes()[0],
            blue: ((blue * 255 + 15) / 31).to_le_bytes()[0],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub colors: Vec<Bgr555>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteEncodingError {
    SizeOverflow { colors: usize },
}

impl std::fmt::Display for PaletteEncodingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid SNES palette encoding: {self:?}")
    }
}

impl std::error::Error for PaletteEncodingError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaletteEditError {
    ColorOutOfRange { index: usize, len: usize },
    WrongRowSize(usize),
}

impl std::fmt::Display for PaletteEditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid palette edit: {self:?}")
    }
}

impl std::error::Error for PaletteEditError {}

impl Palette {
    pub const COLORS_PER_ROW: usize = 16;

    /// Decodes little-endian SNES BGR555 colors.
    ///
    /// # Errors
    ///
    /// Returns the byte length when the input contains a partial color.
    pub fn decode_snes(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() % 2 != 0 {
            return Err(bytes.len());
        }
        Ok(Self {
            colors: bytes
                .chunks_exact(2)
                .map(|pair| Bgr555(u16::from_le_bytes([pair[0], pair[1]])))
                .collect(),
        })
    }

    /// Encodes exact little-endian SNES color words after preflighting aggregate size.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteEncodingError::SizeOverflow`] when two bytes per color cannot be
    /// represented by the platform collection size.
    pub fn encode_snes(&self) -> Result<Vec<u8>, PaletteEncodingError> {
        let encoded_len = encoded_palette_len(self.colors.len())?;
        let mut encoded = Vec::with_capacity(encoded_len);
        for color in &self.colors {
            encoded.extend_from_slice(&color.0.to_le_bytes());
        }
        Ok(encoded)
    }

    #[must_use]
    pub fn row(&self, row: usize) -> Option<&[Bgr555]> {
        let start = row.checked_mul(Self::COLORS_PER_ROW)?;
        self.colors.get(start..start + Self::COLORS_PER_ROW)
    }

    /// Changes one existing palette color without resizing the palette.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteEditError::ColorOutOfRange`] for an invalid index.
    pub fn set_color(&mut self, index: usize, color: Bgr555) -> Result<(), PaletteEditError> {
        let len = self.colors.len();
        let target = self
            .colors
            .get_mut(index)
            .ok_or(PaletteEditError::ColorOutOfRange { index, len })?;
        *target = color;
        Ok(())
    }

    /// Replaces one complete 16-color row without changing palette shape.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteEditError`] unless `colors` has 16 entries and the row already exists.
    pub fn replace_row(&mut self, row: usize, colors: &[Bgr555]) -> Result<(), PaletteEditError> {
        if colors.len() != Self::COLORS_PER_ROW {
            return Err(PaletteEditError::WrongRowSize(colors.len()));
        }
        let start =
            row.checked_mul(Self::COLORS_PER_ROW)
                .ok_or(PaletteEditError::ColorOutOfRange {
                    index: usize::MAX,
                    len: self.colors.len(),
                })?;
        let end =
            start
                .checked_add(Self::COLORS_PER_ROW)
                .ok_or(PaletteEditError::ColorOutOfRange {
                    index: start,
                    len: self.colors.len(),
                })?;
        let len = self.colors.len();
        let target = self
            .colors
            .get_mut(start..end)
            .ok_or(PaletteEditError::ColorOutOfRange { index: start, len })?;
        target.copy_from_slice(colors);
        Ok(())
    }

    /// Finds the closest palette color by squared RGB distance.
    #[must_use]
    pub fn nearest_color(&self, color: Rgb8) -> Option<usize> {
        self.colors
            .iter()
            .enumerate()
            .min_by_key(|(_, candidate)| {
                let candidate = candidate.to_rgb8();
                let red = i32::from(candidate.red) - i32::from(color.red);
                let green = i32::from(candidate.green) - i32::from(color.green);
                let blue = i32::from(candidate.blue) - i32::from(color.blue);
                red * red + green * green + blue * blue
            })
            .map(|(index, _)| index)
    }

    /// Quantizes RGB pixels into palette indexes. Empty palettes cannot be quantized.
    #[must_use]
    pub fn quantize(&self, pixels: &[Rgb8]) -> Option<Vec<usize>> {
        pixels
            .iter()
            .map(|pixel| self.nearest_color(*pixel))
            .collect()
    }
}

fn encoded_palette_len(colors: usize) -> Result<usize, PaletteEncodingError> {
    colors
        .checked_mul(2)
        .ok_or(PaletteEncodingError::SizeOverflow { colors })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_colors_convert() {
        assert_eq!(
            Bgr555::from_rgb8(Rgb8 {
                red: 255,
                green: 0,
                blue: 0
            }),
            Bgr555(31)
        );
        assert_eq!(
            Bgr555(0x7fff).to_rgb8(),
            Rgb8 {
                red: 255,
                green: 255,
                blue: 255
            }
        );
    }

    #[test]
    fn snes_palette_round_trips() {
        let bytes = [0x1f, 0x00, 0xe0, 0x03, 0x00, 0x7c, 0xff, 0x7f];
        let palette = Palette::decode_snes(&bytes).unwrap();
        assert_eq!(palette.encode_snes().unwrap(), bytes);
        assert!(Palette::decode_snes(&[0]).is_err());
    }

    #[test]
    fn encoded_palette_size_is_exact_and_checked() {
        let maximum = usize::MAX / 2;
        assert_eq!(encoded_palette_len(maximum).unwrap(), maximum * 2);
        assert_eq!(
            encoded_palette_len(maximum + 1),
            Err(PaletteEncodingError::SizeOverflow {
                colors: maximum + 1,
            })
        );
    }

    #[test]
    fn nearest_color_is_deterministic() {
        let palette = Palette {
            colors: vec![Bgr555(0), Bgr555(0x001f), Bgr555(0x03e0)],
        };
        assert_eq!(
            palette.quantize(&[
                Rgb8 {
                    red: 250,
                    green: 2,
                    blue: 1,
                },
                Rgb8 {
                    red: 0,
                    green: 240,
                    blue: 5,
                },
            ]),
            Some(vec![1, 2])
        );
        assert_eq!(
            Palette { colors: Vec::new() }.quantize(&[Rgb8::default()]),
            None
        );
    }

    #[test]
    fn bounded_color_and_row_edits_preserve_shape() {
        let mut palette = Palette {
            colors: vec![Bgr555(0); 32],
        };
        palette.set_color(31, Bgr555(7)).unwrap();
        assert_eq!(palette.colors[31], Bgr555(7));
        let row = (0_u16..16).map(Bgr555).collect::<Vec<_>>();
        palette.replace_row(0, &row).unwrap();
        assert_eq!(palette.row(0).unwrap(), row);
        let original = palette.clone();
        assert!(palette.set_color(32, Bgr555(1)).is_err());
        assert!(palette.replace_row(2, &row).is_err());
        assert!(matches!(
            palette.replace_row(0, &row[..15]),
            Err(PaletteEditError::WrongRowSize(15))
        ));
        assert_eq!(palette, original);
    }
}
