use super::{ClipboardError, ClipboardKind, ClipboardPayload};

impl ClipboardPayload {
    #[must_use]
    pub fn from_layer3_tilemap_bytes(bytes: &[u8]) -> Self {
        Self::from_layer3_bytes(ClipboardKind::Layer3TilemapBytes, bytes)
    }

    /// Decodes a lossless selection from the recovered Layer 3 tilemap workspace.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or any record not containing exactly one
    /// byte.
    pub fn to_layer3_tilemap_bytes(&self) -> Result<Vec<u8>, ClipboardError> {
        self.to_layer3_bytes(ClipboardKind::Layer3TilemapBytes)
    }

    #[must_use]
    pub fn from_layer3_remap_bytes(bytes: &[u8]) -> Self {
        Self::from_layer3_bytes(ClipboardKind::Layer3RemapBytes, bytes)
    }

    /// Decodes losslessly selected bytes from the revision-specific Layer 3 remap stream.
    ///
    /// # Errors
    ///
    /// Returns [`ClipboardError`] for the wrong domain or any record not containing exactly one
    /// byte.
    pub fn to_layer3_remap_bytes(&self) -> Result<Vec<u8>, ClipboardError> {
        self.to_layer3_bytes(ClipboardKind::Layer3RemapBytes)
    }

    fn from_layer3_bytes(kind: ClipboardKind, bytes: &[u8]) -> Self {
        Self {
            kind,
            records: bytes.iter().map(|byte| vec![*byte]).collect(),
        }
    }

    fn to_layer3_bytes(&self, kind: ClipboardKind) -> Result<Vec<u8>, ClipboardError> {
        self.require_kind(kind)?;
        self.records
            .iter()
            .enumerate()
            .map(|(index, record)| match record.as_slice() {
                [byte] => Ok(*byte),
                _ => Err(ClipboardError::InvalidRecord {
                    index,
                    length: record.len(),
                }),
            })
            .collect()
    }
}
