use super::{ClipboardError, ClipboardKind, ClipboardPayload};
use lm_graphics::Bgr555;

impl ClipboardPayload {
    #[must_use]
    pub fn from_palette_colors(colors: &[Bgr555]) -> Self {
        Self {
            kind: ClipboardKind::PaletteColors,
            records: colors
                .iter()
                .map(|color| color.0.to_le_bytes().to_vec())
                .collect(),
        }
    }

    /// Decodes SNES palette colors.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or non-two-byte records.
    pub fn to_palette_colors(&self) -> Result<Vec<Bgr555>, ClipboardError> {
        self.require_kind(ClipboardKind::PaletteColors)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let bytes: [u8; 2] =
                    record
                        .as_slice()
                        .try_into()
                        .map_err(|_| ClipboardError::InvalidRecord {
                            index,
                            length: record.len(),
                        })?;
                Ok(Bgr555(u16::from_le_bytes(bytes)))
            })
            .collect()
    }
}
