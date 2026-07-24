use crate::Layer3TilemapGraphicsDescriptor;

pub const LAYER3_TILEMAP_WORKSPACE_LEN: usize = 0x2000;

/// The exact decoded byte workspace consumed by the custom Layer 3 tilemap path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Layer3TilemapWorkspace {
    bytes: Box<[u8; LAYER3_TILEMAP_WORKSPACE_LEN]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer3TilemapWorkspaceError {
    WrongWorkspaceLength(usize),
    GraphicsFileTooShort { required: usize, actual: usize },
}

impl std::fmt::Display for Layer3TilemapWorkspaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Layer 3 tilemap workspace error: {self:?}")
    }
}

impl std::error::Error for Layer3TilemapWorkspaceError {}

impl Default for Layer3TilemapWorkspace {
    fn default() -> Self {
        Self {
            bytes: Box::new([0; LAYER3_TILEMAP_WORKSPACE_LEN]),
        }
    }
}

impl Layer3TilemapWorkspace {
    /// Creates an exact workspace from its lossless byte representation.
    ///
    /// # Errors
    ///
    /// Rejects every input whose length is not the recovered `$2000`-byte capacity.
    pub fn decode(bytes: &[u8]) -> Result<Self, Layer3TilemapWorkspaceError> {
        if bytes.len() != LAYER3_TILEMAP_WORKSPACE_LEN {
            return Err(Layer3TilemapWorkspaceError::WrongWorkspaceLength(
                bytes.len(),
            ));
        }
        let mut exact = Box::new([0; LAYER3_TILEMAP_WORKSPACE_LEN]);
        exact.copy_from_slice(bytes);
        Ok(Self { bytes: exact })
    }

    #[must_use]
    pub fn encoded(&self) -> &[u8; LAYER3_TILEMAP_WORKSPACE_LEN] {
        &self.bytes
    }

    /// Applies the decoded GFX/ExGFX file range selected by the packed native descriptor.
    ///
    /// Bytes outside the selected destination range remain unchanged. Extra decoded source bytes
    /// are ignored, matching the recovered length selector and workspace clipping boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when the decoded graphics file is shorter than the effective selected
    /// range. Failure leaves the workspace unchanged.
    pub fn apply_decoded_file(
        &mut self,
        descriptor: Layer3TilemapGraphicsDescriptor,
        decoded_file: &[u8],
    ) -> Result<(), Layer3TilemapWorkspaceError> {
        let length = usize::from(descriptor.effective_byte_length());
        if decoded_file.len() < length {
            return Err(Layer3TilemapWorkspaceError::GraphicsFileTooShort {
                required: length,
                actual: decoded_file.len(),
            });
        }
        let start = usize::from(descriptor.destination_byte_offset());
        self.bytes[start..start + length].copy_from_slice(&decoded_file[..length]);
        Ok(())
    }

    /// Extracts the exact decoded range written by the native descriptor.
    #[must_use]
    pub fn selected_range(&self, descriptor: Layer3TilemapGraphicsDescriptor) -> &[u8] {
        let start = usize::from(descriptor.destination_byte_offset());
        let length = usize::from(descriptor.effective_byte_length());
        &self.bytes[start..start + length]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(length: u8, offset: u8) -> Layer3TilemapGraphicsDescriptor {
        Layer3TilemapGraphicsDescriptor::new(0x28, length, offset).unwrap()
    }

    #[test]
    fn every_recovered_selector_pair_stays_inside_the_workspace() {
        let source: Vec<_> = (0_u8..=255)
            .cycle()
            .take(LAYER3_TILEMAP_WORKSPACE_LEN)
            .collect();
        for length in 0..4 {
            for offset in 0..4 {
                let descriptor = descriptor(length, offset);
                let mut workspace =
                    Layer3TilemapWorkspace::decode(&vec![0xa5; LAYER3_TILEMAP_WORKSPACE_LEN])
                        .unwrap();
                workspace.apply_decoded_file(descriptor, &source).unwrap();
                assert_eq!(
                    workspace.selected_range(descriptor),
                    &source[..usize::from(descriptor.effective_byte_length())]
                );
            }
        }
    }

    #[test]
    fn offset_three_clips_full_request_and_preserves_prefix() {
        let descriptor = descriptor(0, 3);
        let mut workspace =
            Layer3TilemapWorkspace::decode(&vec![0xa5; LAYER3_TILEMAP_WORKSPACE_LEN]).unwrap();
        workspace
            .apply_decoded_file(descriptor, &vec![0x5a; LAYER3_TILEMAP_WORKSPACE_LEN])
            .unwrap();
        assert_eq!(&workspace.encoded()[..0x1000], &[0xa5; 0x1000]);
        assert_eq!(&workspace.encoded()[0x1000..], &[0x5a; 0x1000]);
    }

    #[test]
    fn short_source_failure_is_atomic_and_zero_length_accepts_empty() {
        let mut workspace =
            Layer3TilemapWorkspace::decode(&vec![0x81; LAYER3_TILEMAP_WORKSPACE_LEN]).unwrap();
        let original = workspace.clone();
        assert_eq!(
            workspace.apply_decoded_file(descriptor(2, 0), &[0; 0x7ff]),
            Err(Layer3TilemapWorkspaceError::GraphicsFileTooShort {
                required: 0x800,
                actual: 0x7ff
            })
        );
        assert_eq!(workspace, original);
        workspace.apply_decoded_file(descriptor(3, 3), &[]).unwrap();
        assert_eq!(workspace, original);
    }

    #[test]
    fn exact_workspace_shape_is_required() {
        assert!(matches!(
            Layer3TilemapWorkspace::decode(&[0; LAYER3_TILEMAP_WORKSPACE_LEN - 1]),
            Err(Layer3TilemapWorkspaceError::WrongWorkspaceLength(0x1fff))
        ));
    }
}
