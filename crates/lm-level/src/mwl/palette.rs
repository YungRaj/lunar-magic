use std::fmt;

/// Lunar Magic's exact MWL palette section.
///
/// The section stores two provenance words, one level backdrop color, and 256 SNES BGR555 words.
/// The 256 stored words are rotated left by one relative to TPL/display order: stored entry 255 is
/// TPL entry 0, while stored entries 0..254 are TPL entries 1..255.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlPaletteSection {
    pub metadata: [u32; 2],
    pub backdrop: u16,
    stored_colors: [u16; Self::COLOR_COUNT],
}

impl MwlPaletteSection {
    pub const COLOR_COUNT: usize = 256;
    pub const METADATA_LEN: usize = 8;
    pub const ENCODED_LEN: usize = Self::METADATA_LEN + 2 + Self::COLOR_COUNT * 2;

    /// Decodes the exact `0x20a`-byte MWL palette representation.
    ///
    /// # Errors
    ///
    /// Returns a length error for both truncation and trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, MwlPaletteSectionError> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(MwlPaletteSectionError::WrongLength {
                expected: Self::ENCODED_LEN,
                actual: bytes.len(),
            });
        }
        let metadata = [read_u32(bytes, 0)?, read_u32(bytes, 4)?];
        let backdrop = read_u16(bytes, 8)?;
        let mut stored_colors = [0; Self::COLOR_COUNT];
        for (color, pair) in stored_colors
            .iter_mut()
            .zip(bytes[Self::METADATA_LEN + 2..].chunks_exact(2))
        {
            *color = u16::from_le_bytes([pair[0], pair[1]]);
        }
        Ok(Self {
            metadata,
            backdrop,
            stored_colors,
        })
    }

    /// Builds a section from the natural 256-entry order used by TPL files and palette editors.
    #[must_use]
    pub fn from_tpl_order(metadata: [u32; 2], backdrop: u16, colors: [u16; 256]) -> Self {
        let stored_colors = std::array::from_fn(|index| colors[(index + 1) % Self::COLOR_COUNT]);
        Self {
            metadata,
            backdrop,
            stored_colors,
        }
    }

    /// Builds a section from the exact backdrop-plus-rotated order used by the native ROM payload.
    #[must_use]
    pub const fn from_stored_order(
        metadata: [u32; 2],
        backdrop: u16,
        stored_colors: [u16; 256],
    ) -> Self {
        Self {
            metadata,
            backdrop,
            stored_colors,
        }
    }

    /// Returns the natural 256-entry order used by TPL files and palette editors.
    #[must_use]
    pub fn tpl_order_colors(&self) -> [u16; Self::COLOR_COUNT] {
        std::array::from_fn(|index| {
            self.stored_colors[(index + Self::COLOR_COUNT - 1) % Self::COLOR_COUNT]
        })
    }

    /// Returns the exact rotated words stored after the backdrop in the MWL section.
    #[must_use]
    pub const fn stored_colors(&self) -> &[u16; Self::COLOR_COUNT] {
        &self.stored_colors
    }

    /// Encodes the exact fixed-size section.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::ENCODED_LEN);
        bytes.extend_from_slice(&self.metadata[0].to_le_bytes());
        bytes.extend_from_slice(&self.metadata[1].to_le_bytes());
        bytes.extend_from_slice(&self.backdrop.to_le_bytes());
        for color in self.stored_colors {
            bytes.extend_from_slice(&color.to_le_bytes());
        }
        bytes
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, MwlPaletteSectionError> {
    let pair = bytes
        .get(offset..offset + 2)
        .ok_or(MwlPaletteSectionError::WrongLength {
            expected: MwlPaletteSection::ENCODED_LEN,
            actual: bytes.len(),
        })?;
    Ok(u16::from_le_bytes([pair[0], pair[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, MwlPaletteSectionError> {
    let word = bytes
        .get(offset..offset + 4)
        .ok_or(MwlPaletteSectionError::WrongLength {
            expected: MwlPaletteSection::ENCODED_LEN,
            actual: bytes.len(),
        })?;
    Ok(u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlPaletteSectionError {
    WrongLength { expected: usize, actual: usize },
}

impl fmt::Display for MwlPaletteSectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid MWL palette section: {self:?}")
    }
}

impl std::error::Error for MwlPaletteSectionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors() -> [u16; 256] {
        std::array::from_fn(|index| u16::try_from(index).unwrap())
    }

    #[test]
    fn tpl_order_is_rotated_into_and_out_of_native_storage() {
        let section = MwlPaletteSection::from_tpl_order([7, 0x10_8031], 0x1234, colors());
        assert_eq!(section.stored_colors()[0], 1);
        assert_eq!(section.stored_colors()[254], 255);
        assert_eq!(section.stored_colors()[255], 0);
        assert_eq!(section.tpl_order_colors(), colors());
        assert_eq!(
            MwlPaletteSection::decode(&section.encode()).unwrap(),
            section
        );
    }

    #[test]
    fn exact_shape_rejects_truncation_and_trailing_data() {
        let section = MwlPaletteSection::from_tpl_order([0; 2], 0, colors()).encode();
        for length in 0..=MwlPaletteSection::ENCODED_LEN + 1 {
            if length != MwlPaletteSection::ENCODED_LEN {
                let mut candidate = section[..length.min(section.len())].to_vec();
                candidate.resize(length, 0);
                assert!(MwlPaletteSection::decode(&candidate).is_err());
            }
        }
    }
}
