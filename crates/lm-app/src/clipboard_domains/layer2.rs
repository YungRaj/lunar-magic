use super::{ClipboardError, ClipboardKind, ClipboardPayload};

impl ClipboardPayload {
    /// Encodes one bounded rectangular Layer 2 Map16 selection in visual row-major order.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError::InvalidRecord`] unless both dimensions are in `1..=32` and the
    /// word count exactly equals `width * height`.
    pub fn from_layer2_tilemap_selection(
        width: u8,
        height: u8,
        words: &[u16],
    ) -> Result<Self, ClipboardError> {
        let expected = usize::from(width)
            .checked_mul(usize::from(height))
            .filter(|_| (1..=32).contains(&width) && (1..=32).contains(&height));
        if expected != Some(words.len()) {
            return Err(ClipboardError::InvalidRecord {
                index: 0,
                length: words.len(),
            });
        }
        let mut record = Vec::with_capacity(2 + words.len() * 2);
        record.extend_from_slice(&[width, height]);
        for word in words {
            record.extend_from_slice(&word.to_le_bytes());
        }
        Self::new(ClipboardKind::Layer2TilemapSelection, vec![record])
    }

    /// Decodes one rectangular Layer 2 Map16 selection as width, height, and visual-row words.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for another clipboard kind, the wrong record count, invalid
    /// dimensions, or a payload whose word count does not match its dimensions.
    pub fn to_layer2_tilemap_selection(&self) -> Result<(u8, u8, Vec<u16>), ClipboardError> {
        self.require_kind(ClipboardKind::Layer2TilemapSelection)?;
        let [record] = self.records.as_slice() else {
            return Err(ClipboardError::InvalidRecord {
                index: 0,
                length: self.records.len(),
            });
        };
        let Some((&width, tail)) = record.split_first() else {
            return Err(invalid(record.len()));
        };
        let Some((&height, bytes)) = tail.split_first() else {
            return Err(invalid(record.len()));
        };
        let expected = usize::from(width)
            .checked_mul(usize::from(height))
            .and_then(|words| words.checked_mul(2))
            .filter(|_| (1..=32).contains(&width) && (1..=32).contains(&height));
        if expected != Some(bytes.len()) {
            return Err(invalid(record.len()));
        }
        let words = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        Ok((width, height, words))
    }
}

fn invalid(length: usize) -> ClipboardError {
    ClipboardError::InvalidRecord { index: 0, length }
}
