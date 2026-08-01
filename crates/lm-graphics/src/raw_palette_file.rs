use crate::{Bgr555, Palette};
use std::fmt;

/// Lunar Magic's extension-independent raw SNES palette file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawSnesPaletteFile {
    pub palette: Palette,
}

impl RawSnesPaletteFile {
    pub const COLOR_COUNT: usize = 0x101;
    pub const FILE_LEN: usize = Self::COLOR_COUNT * 2;

    /// Decodes exactly 257 little-endian SNES BGR555 words.
    ///
    /// # Errors
    ///
    /// Returns a typed length or color-data error for malformed input.
    pub fn decode(bytes: &[u8]) -> Result<Self, RawPaletteFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(RawPaletteFileError::WrongPaletteLength {
                expected: Self::FILE_LEN,
                actual: bytes.len(),
            });
        }
        Ok(Self {
            palette: Palette::decode_snes(bytes).map_err(RawPaletteFileError::ColorData)?,
        })
    }

    /// Encodes exactly 257 little-endian SNES BGR555 words.
    ///
    /// # Errors
    ///
    /// Returns a color-count error unless the model has the recovered fixed shape.
    pub fn encode(&self) -> Result<Vec<u8>, RawPaletteFileError> {
        if self.palette.colors.len() != Self::COLOR_COUNT {
            return Err(RawPaletteFileError::WrongColorCount(
                self.palette.colors.len(),
            ));
        }
        self.palette
            .encode_snes()
            .map_err(|_| RawPaletteFileError::WrongColorCount(self.palette.colors.len()))
    }
}

/// A lossless `.palmask` selection sidecar. Lunar Magic treats zero as retained and every nonzero
/// byte as selected, so noncanonical nonzero values are preserved rather than normalized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteMaskFile {
    entries: Vec<u8>,
}

impl PaletteMaskFile {
    pub const ENTRY_COUNT: usize = RawSnesPaletteFile::COLOR_COUNT;
    pub const FILE_LEN: usize = Self::ENTRY_COUNT;

    /// Decodes exactly 257 selector bytes.
    ///
    /// # Errors
    ///
    /// Returns a length error for truncation or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, RawPaletteFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(RawPaletteFileError::WrongMaskLength {
                expected: Self::FILE_LEN,
                actual: bytes.len(),
            });
        }
        Ok(Self {
            entries: bytes.to_vec(),
        })
    }

    /// Constructs a mask with all entries selected, matching Lunar Magic when no `.palmask` sidecar
    /// exists.
    #[must_use]
    pub fn all_selected() -> Self {
        Self {
            entries: vec![1; Self::ENTRY_COUNT],
        }
    }

    #[must_use]
    pub fn entries(&self) -> &[u8] {
        &self.entries
    }

    #[must_use]
    pub fn is_selected(&self, index: usize) -> Option<bool> {
        self.entries.get(index).map(|value| *value != 0)
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        self.entries.clone()
    }
}

/// Applies a raw import and its optional selection sidecar exactly as the recovered loader does.
///
/// Selected source entries replace the working palette. Afterwards, each selected first color in
/// the sixteen ordinary rows is cleared to zero; the separate 257th color is not row-zero data.
/// Validation completes before mutation.
///
/// # Errors
///
/// Returns a shape error unless source, destination, and mask all have exactly 257 entries.
pub fn apply_raw_palette_import(
    destination: &mut Palette,
    source: &RawSnesPaletteFile,
    mask: &PaletteMaskFile,
) -> Result<(), RawPaletteFileError> {
    if destination.colors.len() != RawSnesPaletteFile::COLOR_COUNT {
        return Err(RawPaletteFileError::WrongDestinationColorCount(
            destination.colors.len(),
        ));
    }
    if source.palette.colors.len() != RawSnesPaletteFile::COLOR_COUNT {
        return Err(RawPaletteFileError::WrongColorCount(
            source.palette.colors.len(),
        ));
    }
    if mask.entries.len() != PaletteMaskFile::ENTRY_COUNT {
        return Err(RawPaletteFileError::WrongMaskLength {
            expected: PaletteMaskFile::ENTRY_COUNT,
            actual: mask.entries.len(),
        });
    }
    let mut staged = destination.clone();
    for (index, source_color) in source.palette.colors.iter().enumerate() {
        if mask.entries[index] != 0 {
            staged.colors[index] = *source_color;
        }
    }
    for row in 0..16 {
        let index = row * 16;
        if mask.entries[index] != 0 {
            staged.colors[index] = Bgr555(0);
        }
    }
    *destination = staged;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RawPaletteFileError {
    WrongPaletteLength { expected: usize, actual: usize },
    WrongMaskLength { expected: usize, actual: usize },
    WrongColorCount(usize),
    WrongDestinationColorCount(usize),
    ColorData(usize),
}

impl fmt::Display for RawPaletteFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid Lunar Magic raw palette file: {self:?}")
    }
}

impl std::error::Error for RawPaletteFileError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> RawSnesPaletteFile {
        RawSnesPaletteFile {
            palette: Palette {
                colors: (0_u16..=256).map(Bgr555).collect(),
            },
        }
    }

    #[test]
    fn raw_palette_and_noncanonical_mask_round_trip_exactly() {
        let source = source();
        assert_eq!(
            RawSnesPaletteFile::decode(&source.encode().unwrap()).unwrap(),
            source
        );
        let entries: Vec<_> = (0..PaletteMaskFile::ENTRY_COUNT)
            .map(|index| if index % 3 == 0 { 0 } else { 0x80 })
            .collect();
        let mask = PaletteMaskFile::decode(&entries).unwrap();
        assert!(!mask.is_selected(0).unwrap());
        assert!(mask.is_selected(1).unwrap());
        assert_eq!(mask.encode(), entries);
    }

    #[test]
    fn every_wrong_file_length_is_rejected() {
        for length in 0..=RawSnesPaletteFile::FILE_LEN + 1 {
            if length != RawSnesPaletteFile::FILE_LEN {
                assert!(RawSnesPaletteFile::decode(&vec![0; length]).is_err());
            }
        }
        for length in 0..=PaletteMaskFile::FILE_LEN + 1 {
            if length != PaletteMaskFile::FILE_LEN {
                assert!(PaletteMaskFile::decode(&vec![0; length]).is_err());
            }
        }
    }

    #[test]
    fn masked_import_preserves_unselected_entries_and_clears_selected_row_zeroes() {
        let source = source();
        let mut destination = Palette {
            colors: vec![Bgr555(0x7fff); RawSnesPaletteFile::COLOR_COUNT],
        };
        let mut entries = vec![0; PaletteMaskFile::ENTRY_COUNT];
        entries[0] = 1;
        entries[1] = 2;
        entries[16] = 1;
        entries[256] = 1;
        let mask = PaletteMaskFile::decode(&entries).unwrap();
        apply_raw_palette_import(&mut destination, &source, &mask).unwrap();
        assert_eq!(destination.colors[0], Bgr555(0));
        assert_eq!(destination.colors[1], Bgr555(1));
        assert_eq!(destination.colors[2], Bgr555(0x7fff));
        assert_eq!(destination.colors[16], Bgr555(0));
        assert_eq!(destination.colors[256], Bgr555(256));
    }

    #[test]
    fn destination_shape_failure_is_atomic() {
        let mut destination = Palette {
            colors: vec![Bgr555(9); 256],
        };
        let original = destination.clone();
        assert_eq!(
            apply_raw_palette_import(
                &mut destination,
                &source(),
                &PaletteMaskFile::all_selected()
            ),
            Err(RawPaletteFileError::WrongDestinationColorCount(256))
        );
        assert_eq!(destination, original);
    }
}
