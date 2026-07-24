use crate::Palette;
use std::fmt;

/// A versioned interchange file containing exact little-endian SNES BGR555 words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteInterchangeFile {
    pub source_palette: u16,
    pub palette: Palette,
}

impl PaletteInterchangeFile {
    pub const MAGIC: [u8; 8] = *b"LMPAL1\0\0";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 16;
    pub const MAX_COLORS: usize = 0x1_0000;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN + Self::MAX_COLORS * 2;

    /// Encodes a canonical exact-color palette file.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteInterchangeError`] when the color count exceeds the format limit.
    pub fn encode(&self) -> Result<Vec<u8>, PaletteInterchangeError> {
        validate_count(self.palette.colors.len())?;
        let count = u32::try_from(self.palette.colors.len())
            .map_err(|_| PaletteInterchangeError::Overflow)?;
        let payload = self
            .palette
            .encode_snes()
            .map_err(|_| PaletteInterchangeError::Overflow)?;
        let mut bytes = Vec::with_capacity(Self::HEADER_LEN + payload.len());
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.source_palette.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&payload);
        Ok(bytes)
    }

    /// Decodes a palette file and rejects truncation and trailing bytes.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteInterchangeError`] for invalid framing, count, length, or color data.
    pub fn decode(bytes: &[u8]) -> Result<Self, PaletteInterchangeError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(PaletteInterchangeError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(PaletteInterchangeError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(PaletteInterchangeError::UnsupportedVersion(version));
        }
        let source_palette = u16::from_le_bytes([header[10], header[11]]);
        let count = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| PaletteInterchangeError::Overflow)?;
        validate_count(count)?;
        let payload_len = count
            .checked_mul(2)
            .ok_or(PaletteInterchangeError::Overflow)?;
        let expected = Self::HEADER_LEN
            .checked_add(payload_len)
            .ok_or(PaletteInterchangeError::Overflow)?;
        if bytes.len() != expected {
            return Err(PaletteInterchangeError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self {
            source_palette,
            palette: Palette::decode_snes(&bytes[Self::HEADER_LEN..])
                .map_err(PaletteInterchangeError::ColorData)?,
        })
    }
}

fn validate_count(count: usize) -> Result<(), PaletteInterchangeError> {
    if count > PaletteInterchangeFile::MAX_COLORS {
        Err(PaletteInterchangeError::TooManyColors(count))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteInterchangeError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    TooManyColors(usize),
    WrongLength { expected: usize, actual: usize },
    ColorData(usize),
    Overflow,
}

impl fmt::Display for PaletteInterchangeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid palette interchange file: {self:?}")
    }
}

impl std::error::Error for PaletteInterchangeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bgr555;

    fn file() -> PaletteInterchangeFile {
        PaletteInterchangeFile {
            source_palette: 0x105,
            palette: Palette {
                colors: (0_u16..256).map(Bgr555).collect(),
            },
        }
    }

    #[test]
    fn exact_words_and_source_slot_round_trip() {
        let file = file();
        assert_eq!(
            PaletteInterchangeFile::decode(&file.encode().unwrap()).unwrap(),
            file
        );
    }

    #[test]
    fn wrong_version_count_and_trailing_data_are_rejected() {
        let bytes = file().encode().unwrap();
        let mut version = bytes.clone();
        version[8..10].copy_from_slice(&2_u16.to_le_bytes());
        assert!(matches!(
            PaletteInterchangeFile::decode(&version),
            Err(PaletteInterchangeError::UnsupportedVersion(2))
        ));
        let mut count = bytes.clone();
        count[12..16].copy_from_slice(&0x1_0001_u32.to_le_bytes());
        assert!(matches!(
            PaletteInterchangeFile::decode(&count),
            Err(PaletteInterchangeError::TooManyColors(0x1_0001))
        ));
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            PaletteInterchangeFile::decode(&trailing),
            Err(PaletteInterchangeError::WrongLength { .. })
        ));
    }
}
