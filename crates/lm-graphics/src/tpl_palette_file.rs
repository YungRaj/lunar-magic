use crate::Palette;
use std::fmt;

/// Lunar Magic's version-2 TPL palette interchange: native SNES BGR555 words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TplPaletteFile {
    pub palette: Palette,
}

impl TplPaletteFile {
    pub const MAGIC: [u8; 3] = *b"TPL";
    pub const VERSION: u8 = 2;
    pub const COLOR_COUNT: usize = 256;
    pub const HEADER_LEN: usize = 4;
    pub const FILE_LEN: usize = Self::HEADER_LEN + Self::COLOR_COUNT * 2;

    /// Decodes an exact version-2 TPL file.
    ///
    /// # Errors
    ///
    /// Returns a typed error for truncation/trailing data, wrong magic/version, or malformed SNES
    /// color words.
    pub fn decode(bytes: &[u8]) -> Result<Self, TplPaletteFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(TplPaletteFileError::WrongLength {
                expected: Self::FILE_LEN,
                actual: bytes.len(),
            });
        }
        if bytes[..3] != Self::MAGIC {
            return Err(TplPaletteFileError::WrongMagic);
        }
        if bytes[3] != Self::VERSION {
            return Err(TplPaletteFileError::UnsupportedVersion(bytes[3]));
        }
        Ok(Self {
            palette: Palette::decode_snes(&bytes[Self::HEADER_LEN..])
                .map_err(TplPaletteFileError::ColorData)?,
        })
    }

    /// Encodes exactly 256 colors in Lunar Magic's version-2 TPL representation.
    ///
    /// # Errors
    ///
    /// Returns [`TplPaletteFileError::WrongColorCount`] unless the palette contains exactly 256
    /// colors.
    pub fn encode(&self) -> Result<Vec<u8>, TplPaletteFileError> {
        if self.palette.colors.len() != Self::COLOR_COUNT {
            return Err(TplPaletteFileError::WrongColorCount(
                self.palette.colors.len(),
            ));
        }
        let payload = self
            .palette
            .encode_snes()
            .map_err(|_| TplPaletteFileError::WrongColorCount(self.palette.colors.len()))?;
        let mut bytes = Vec::with_capacity(Self::FILE_LEN);
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.push(Self::VERSION);
        bytes.extend_from_slice(&payload);
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TplPaletteFileError {
    WrongLength { expected: usize, actual: usize },
    WrongMagic,
    UnsupportedVersion(u8),
    WrongColorCount(usize),
    ColorData(usize),
}

impl fmt::Display for TplPaletteFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Lunar Magic TPL palette file: {self:?}")
    }
}

impl std::error::Error for TplPaletteFileError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Bgr555;

    fn file() -> TplPaletteFile {
        TplPaletteFile {
            palette: Palette {
                colors: (0_u16..256).map(Bgr555).collect(),
            },
        }
    }

    #[test]
    fn version_two_round_trips_exact_words() {
        let file = file();
        let encoded = file.encode().unwrap();
        assert_eq!(&encoded[..4], b"TPL\x02");
        assert_eq!(TplPaletteFile::decode(&encoded).unwrap(), file);
    }

    #[test]
    fn framing_version_count_and_every_wrong_length_are_rejected() {
        let encoded = file().encode().unwrap();
        for length in 0..=TplPaletteFile::FILE_LEN + 1 {
            if length != TplPaletteFile::FILE_LEN {
                assert!(TplPaletteFile::decode(&vec![0; length]).is_err());
            }
        }
        let mut magic = encoded.clone();
        magic[0] = b'X';
        assert_eq!(
            TplPaletteFile::decode(&magic),
            Err(TplPaletteFileError::WrongMagic)
        );
        let mut version = encoded;
        version[3] = 0;
        assert_eq!(
            TplPaletteFile::decode(&version),
            Err(TplPaletteFileError::UnsupportedVersion(0))
        );
        assert_eq!(
            TplPaletteFile {
                palette: Palette { colors: vec![] }
            }
            .encode(),
            Err(TplPaletteFileError::WrongColorCount(0))
        );
    }
}
