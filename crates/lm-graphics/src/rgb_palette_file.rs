use crate::{Bgr555, Palette, PaletteMaskFile, Rgb8};
use std::fmt;

/// The two five-bit-to-eight-bit channel representations detected by Lunar Magic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RgbChannelExpansion {
    /// Five significant bits followed by three zero bits.
    HighBits,
    /// Five significant bits followed by a copy of their upper three bits.
    ReplicatedBits,
}

/// A raw 256-color RGB24 `.pal` file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RgbPaletteFile {
    pub colors: Vec<Rgb8>,
    pub detected_expansion: RgbChannelExpansion,
}

impl RgbPaletteFile {
    pub const COLOR_COUNT: usize = 256;
    pub const FILE_LEN: usize = Self::COLOR_COUNT * 3;

    /// Decodes exact RGB triplets and applies Lunar Magic's expansion detector.
    ///
    /// # Errors
    ///
    /// Returns a length error unless the file contains precisely 768 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, RgbPaletteFileError> {
        Self::decode_selected(bytes, None)
    }

    /// Decodes RGB triplets while limiting expansion evidence to `.palm`-selected entries, exactly
    /// like Lunar Magic's importer. RGB bytes remain lossless regardless of selection.
    ///
    /// # Errors
    ///
    /// Returns a length error unless the RGB file contains precisely 768 bytes.
    pub fn decode_with_mask(
        bytes: &[u8],
        mask: &PaletteMaskFile,
    ) -> Result<Self, RgbPaletteFileError> {
        Self::decode_selected(bytes, Some(mask))
    }

    fn decode_selected(
        bytes: &[u8],
        mask: Option<&PaletteMaskFile>,
    ) -> Result<Self, RgbPaletteFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(RgbPaletteFileError::WrongLength {
                expected: Self::FILE_LEN,
                actual: bytes.len(),
            });
        }
        let colors: Vec<_> = bytes
            .chunks_exact(3)
            .map(|rgb| Rgb8 {
                red: rgb[0],
                green: rgb[1],
                blue: rgb[2],
            })
            .collect();
        Ok(Self {
            detected_expansion: detect_expansion(&colors, mask),
            colors,
        })
    }

    /// Encodes the retained RGB bytes exactly; detected expansion is semantic metadata and does
    /// not rewrite arbitrary source channels.
    ///
    /// # Errors
    ///
    /// Returns a color-count error unless the file contains exactly 256 colors.
    pub fn encode(&self) -> Result<Vec<u8>, RgbPaletteFileError> {
        if self.colors.len() != Self::COLOR_COUNT {
            return Err(RgbPaletteFileError::WrongColorCount(self.colors.len()));
        }
        let mut bytes = Vec::with_capacity(Self::FILE_LEN);
        for color in &self.colors {
            bytes.extend_from_slice(&[color.red, color.green, color.blue]);
        }
        Ok(bytes)
    }

    /// Converts every RGB triplet using the recovered detected expansion convention.
    #[must_use]
    pub fn to_snes_palette(&self) -> Palette {
        Palette {
            colors: self
                .colors
                .iter()
                .map(|color| rgb_to_bgr555(*color, self.detected_expansion))
                .collect(),
        }
    }

    /// Creates canonical RGB triplets from exactly 256 SNES colors using a selected convention.
    ///
    /// # Errors
    ///
    /// Returns a color-count error unless `palette` contains exactly 256 colors.
    pub fn from_snes_palette(
        palette: &Palette,
        expansion: RgbChannelExpansion,
    ) -> Result<Self, RgbPaletteFileError> {
        if palette.colors.len() != Self::COLOR_COUNT {
            return Err(RgbPaletteFileError::WrongColorCount(palette.colors.len()));
        }
        Ok(Self {
            colors: palette
                .colors
                .iter()
                .map(|color| expand_bgr555(*color, expansion))
                .collect(),
            detected_expansion: expansion,
        })
    }
}

fn detect_expansion(colors: &[Rgb8], mask: Option<&PaletteMaskFile>) -> RgbChannelExpansion {
    let mut evidence = 0_usize;
    let mut high_bits_only = 0_usize;
    for (index, color) in colors.iter().enumerate() {
        if mask.is_some_and(|mask| !mask.is_selected(index).unwrap_or(false)) {
            continue;
        }
        let channels = [color.red, color.green, color.blue];
        if channels.iter().any(|channel| channel & 7 != 0) {
            evidence += 1;
        } else if channels.iter().any(|channel| channel & 0xe0 != 0) {
            evidence += 1;
            high_bits_only += 1;
        }
    }
    if high_bits_only.saturating_mul(2) > evidence {
        RgbChannelExpansion::HighBits
    } else {
        RgbChannelExpansion::ReplicatedBits
    }
}

fn rgb_to_bgr555(color: Rgb8, expansion: RgbChannelExpansion) -> Bgr555 {
    let convert = |channel| match expansion {
        RgbChannelExpansion::HighBits => channel >> 3,
        RgbChannelExpansion::ReplicatedBits => nearest_replicated_level(channel),
    };
    let red = u16::from(convert(color.red));
    let green = u16::from(convert(color.green));
    let blue = u16::from(convert(color.blue));
    Bgr555(red | (green << 5) | (blue << 10))
}

fn nearest_replicated_level(channel: u8) -> u8 {
    (0_u8..32)
        .min_by_key(|level| {
            let expanded = (*level << 3) | (*level >> 2);
            (
                u8::abs_diff(channel, expanded),
                u8::MAX.saturating_sub(*level),
            )
        })
        .unwrap_or(0)
}

fn expand_bgr555(color: Bgr555, expansion: RgbChannelExpansion) -> Rgb8 {
    let expand = |value: u16| {
        let value = value.to_le_bytes()[0] & 31;
        match expansion {
            RgbChannelExpansion::HighBits => value << 3,
            RgbChannelExpansion::ReplicatedBits => (value << 3) | (value >> 2),
        }
    };
    Rgb8 {
        red: expand(color.0),
        green: expand(color.0 >> 5),
        blue: expand(color.0 >> 10),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RgbPaletteFileError {
    WrongLength { expected: usize, actual: usize },
    WrongColorCount(usize),
}

impl fmt::Display for RgbPaletteFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Lunar Magic RGB palette file: {self:?}")
    }
}

impl std::error::Error for RgbPaletteFileError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            colors: (0_u16..256).map(Bgr555).collect(),
        }
    }

    #[test]
    fn both_canonical_expansions_detect_and_convert_exactly() {
        for expansion in [
            RgbChannelExpansion::HighBits,
            RgbChannelExpansion::ReplicatedBits,
        ] {
            let file = RgbPaletteFile::from_snes_palette(&palette(), expansion).unwrap();
            let decoded = RgbPaletteFile::decode(&file.encode().unwrap()).unwrap();
            assert_eq!(decoded.detected_expansion, expansion);
            assert_eq!(decoded.to_snes_palette(), palette());
        }
    }

    #[test]
    fn detector_matches_recovered_majority_rule_and_preserves_bytes() {
        let mut bytes = vec![0; RgbPaletteFile::FILE_LEN];
        bytes[..3].copy_from_slice(&[0xf8, 0, 0]);
        let file = RgbPaletteFile::decode(&bytes).unwrap();
        assert_eq!(file.detected_expansion, RgbChannelExpansion::HighBits);
        assert_eq!(file.encode().unwrap(), bytes);

        bytes[3..6].copy_from_slice(&[0xff, 0, 0]);
        bytes[6..9].copy_from_slice(&[0xff, 0, 0]);
        assert_eq!(
            RgbPaletteFile::decode(&bytes).unwrap().detected_expansion,
            RgbChannelExpansion::ReplicatedBits
        );
    }

    #[test]
    fn selection_mask_limits_detector_evidence() {
        let mut bytes = vec![0; RgbPaletteFile::FILE_LEN];
        bytes[..3].copy_from_slice(&[0xf8, 0, 0]);
        bytes[3..6].copy_from_slice(&[0xff, 0, 0]);
        bytes[6..9].copy_from_slice(&[0xff, 0, 0]);
        assert_eq!(
            RgbPaletteFile::decode(&bytes).unwrap().detected_expansion,
            RgbChannelExpansion::ReplicatedBits
        );
        let mut selected = vec![0; PaletteMaskFile::FILE_LEN];
        selected[0] = 1;
        let mask = PaletteMaskFile::decode(&selected).unwrap();
        assert_eq!(
            RgbPaletteFile::decode_with_mask(&bytes, &mask)
                .unwrap()
                .detected_expansion,
            RgbChannelExpansion::HighBits
        );
    }

    #[test]
    fn replicated_quantizer_chooses_the_nearest_level_with_upward_ties() {
        for channel in 0_u8..=255 {
            let actual = nearest_replicated_level(channel);
            let actual_expanded = (actual << 3) | (actual >> 2);
            for candidate in 0_u8..32 {
                let expanded = (candidate << 3) | (candidate >> 2);
                assert!(
                    u8::abs_diff(channel, actual_expanded) < u8::abs_diff(channel, expanded)
                        || (u8::abs_diff(channel, actual_expanded)
                            == u8::abs_diff(channel, expanded)
                            && actual >= candidate)
                );
            }
        }
    }

    #[test]
    fn every_wrong_length_and_color_count_is_rejected() {
        for length in 0..=RgbPaletteFile::FILE_LEN + 1 {
            if length != RgbPaletteFile::FILE_LEN {
                assert!(RgbPaletteFile::decode(&vec![0; length]).is_err());
            }
        }
        assert!(
            RgbPaletteFile::from_snes_palette(
                &Palette { colors: vec![] },
                RgbChannelExpansion::HighBits
            )
            .is_err()
        );
    }
}
