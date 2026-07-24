use crate::Palette;
use std::fmt;

/// The two complete `.smwpal` working-palette layouts recovered from Lunar Magic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmwPaletteBackend {
    Legacy,
    Expanded,
}

/// A lossless native Lunar Magic `.smwpal` file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmwPaletteFile {
    backend: SmwPaletteBackend,
    palette_bytes: Vec<u8>,
    auxiliary: Vec<u8>,
}

impl SmwPaletteFile {
    pub const LEGACY_PALETTE_LEN: usize = 0x7e2;
    pub const EXPANDED_PALETTE_LEN: usize = 0x800;
    pub const EXPANDED_AUXILIARY_LEN: usize = 0x10;
    pub const EXPANDED_FILE_LEN: usize = Self::EXPANDED_PALETTE_LEN + Self::EXPANDED_AUXILIARY_LEN;
    pub const MAX_FILE_LEN: usize = Self::EXPANDED_FILE_LEN;

    /// Decodes one exact native layout, rejecting truncation and trailing bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SmwPaletteFileError::WrongLength`] unless the file is exactly `0x7e2` bytes for
    /// the legacy backend or `0x810` bytes for the expanded backend.
    pub fn decode(bytes: &[u8]) -> Result<Self, SmwPaletteFileError> {
        match bytes.len() {
            Self::LEGACY_PALETTE_LEN => Ok(Self {
                backend: SmwPaletteBackend::Legacy,
                palette_bytes: bytes.to_vec(),
                auxiliary: Vec::new(),
            }),
            Self::EXPANDED_FILE_LEN => Ok(Self {
                backend: SmwPaletteBackend::Expanded,
                palette_bytes: bytes[..Self::EXPANDED_PALETTE_LEN].to_vec(),
                auxiliary: bytes[Self::EXPANDED_PALETTE_LEN..].to_vec(),
            }),
            actual => Err(SmwPaletteFileError::WrongLength { actual }),
        }
    }

    /// Constructs a legacy-backend file from its exact SNES-color byte region.
    ///
    /// # Errors
    ///
    /// Returns a length error unless the region is exactly `0x7e2` bytes.
    pub fn legacy(palette_bytes: Vec<u8>) -> Result<Self, SmwPaletteFileError> {
        if palette_bytes.len() != Self::LEGACY_PALETTE_LEN {
            return Err(SmwPaletteFileError::WrongPaletteLength {
                backend: SmwPaletteBackend::Legacy,
                actual: palette_bytes.len(),
            });
        }
        Ok(Self {
            backend: SmwPaletteBackend::Legacy,
            palette_bytes,
            auxiliary: Vec::new(),
        })
    }

    /// Constructs an expanded-backend file from its two exact exported regions.
    ///
    /// # Errors
    ///
    /// Returns a length error unless the main and auxiliary regions are exactly `0x800` and
    /// `0x10` bytes respectively.
    pub fn expanded(
        palette_bytes: Vec<u8>,
        auxiliary: Vec<u8>,
    ) -> Result<Self, SmwPaletteFileError> {
        if palette_bytes.len() != Self::EXPANDED_PALETTE_LEN {
            return Err(SmwPaletteFileError::WrongPaletteLength {
                backend: SmwPaletteBackend::Expanded,
                actual: palette_bytes.len(),
            });
        }
        if auxiliary.len() != Self::EXPANDED_AUXILIARY_LEN {
            return Err(SmwPaletteFileError::WrongAuxiliaryLength {
                actual: auxiliary.len(),
            });
        }
        Ok(Self {
            backend: SmwPaletteBackend::Expanded,
            palette_bytes,
            auxiliary,
        })
    }

    #[must_use]
    pub fn backend(&self) -> SmwPaletteBackend {
        self.backend
    }

    #[must_use]
    pub fn palette_bytes(&self) -> &[u8] {
        &self.palette_bytes
    }

    #[must_use]
    pub fn auxiliary_bytes(&self) -> &[u8] {
        &self.auxiliary
    }

    /// Decodes the complete main region as little-endian SNES BGR555 words.
    ///
    /// # Errors
    ///
    /// Returns a color-data error if an internally constructed file violates the even-byte
    /// palette invariant.
    pub fn palette(&self) -> Result<Palette, SmwPaletteFileError> {
        Palette::decode_snes(&self.palette_bytes).map_err(SmwPaletteFileError::ColorData)
    }

    /// Re-encodes the exact native file ordering. Expanded files store the `0x800`-byte main
    /// palette first and the separately owned `0x10`-byte auxiliary region second.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.palette_bytes.len() + self.auxiliary.len());
        bytes.extend_from_slice(&self.palette_bytes);
        bytes.extend_from_slice(&self.auxiliary);
        bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmwPaletteFileError {
    WrongLength {
        actual: usize,
    },
    WrongPaletteLength {
        backend: SmwPaletteBackend,
        actual: usize,
    },
    WrongAuxiliaryLength {
        actual: usize,
    },
    ColorData(usize),
}

impl fmt::Display for SmwPaletteFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Lunar Magic .smwpal file: {self:?}")
    }
}

impl std::error::Error for SmwPaletteFileError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_recovered_backend_layouts_round_trip_exactly() {
        let legacy: Vec<_> = (0..SmwPaletteFile::LEGACY_PALETTE_LEN)
            .map(|index| index.to_le_bytes()[0])
            .collect();
        let legacy_file = SmwPaletteFile::decode(&legacy).unwrap();
        assert_eq!(legacy_file.backend(), SmwPaletteBackend::Legacy);
        assert!(legacy_file.auxiliary_bytes().is_empty());
        assert_eq!(legacy_file.palette().unwrap().colors.len(), 0x3f1);
        assert_eq!(legacy_file.encode(), legacy);

        let expanded: Vec<_> = (0..SmwPaletteFile::EXPANDED_FILE_LEN)
            .map(|index| index.wrapping_mul(7).to_le_bytes()[0])
            .collect();
        let expanded_file = SmwPaletteFile::decode(&expanded).unwrap();
        assert_eq!(expanded_file.backend(), SmwPaletteBackend::Expanded);
        assert_eq!(expanded_file.palette_bytes().len(), 0x800);
        assert_eq!(expanded_file.auxiliary_bytes(), &expanded[0x800..]);
        assert_eq!(expanded_file.palette().unwrap().colors.len(), 0x400);
        assert_eq!(expanded_file.encode(), expanded);
    }

    #[test]
    fn every_other_length_and_bad_constructor_shape_is_rejected() {
        for length in 0..=SmwPaletteFile::MAX_FILE_LEN + 1 {
            if !matches!(
                length,
                SmwPaletteFile::LEGACY_PALETTE_LEN | SmwPaletteFile::EXPANDED_FILE_LEN
            ) {
                assert!(SmwPaletteFile::decode(&vec![0; length]).is_err());
            }
        }
        assert!(SmwPaletteFile::legacy(vec![0; 0x7e1]).is_err());
        assert!(SmwPaletteFile::expanded(vec![0; 0x800], vec![0; 15]).is_err());
    }
}
