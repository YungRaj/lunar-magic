use super::{MwlError, payload_section_len, read_u32};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MwlPayloadSection {
    pub metadata: [u32; 2],
    pub payload: Vec<u8>,
}

impl MwlPayloadSection {
    pub const METADATA_LEN: usize = 8;

    /// Decodes the common two-word prefix used by Layer 1, Layer 2, sprites, palettes, secondary
    /// exits, and `ExAnimation` sections.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Truncated`] if the section is shorter than eight bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, MwlError> {
        if bytes.len() < Self::METADATA_LEN {
            return Err(MwlError::Truncated {
                offset: 0,
                needed: Self::METADATA_LEN,
            });
        }
        Ok(Self {
            metadata: [read_u32(bytes, 0)?, read_u32(bytes, 4)?],
            payload: bytes[Self::METADATA_LEN..].to_vec(),
        })
    }

    /// Encodes the common two-word prefix and payload after exact aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Overflow`] if metadata and payload length cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, MwlError> {
        let mut bytes = Vec::with_capacity(payload_section_len(self.payload.len())?);
        bytes.extend_from_slice(&self.metadata[0].to_le_bytes());
        bytes.extend_from_slice(&self.metadata[1].to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlLevelHeaderSection(pub [u8; Self::ENCODED_LEN]);

impl MwlLevelHeaderSection {
    pub const ENCODED_LEN: usize = 0x40;

    /// Decodes the fixed-size MWL level-settings section without interpreting unproven fields.
    ///
    /// # Errors
    ///
    /// Returns [`MwlError::Truncated`] unless exactly 64 bytes are supplied.
    pub fn decode(bytes: &[u8]) -> Result<Self, MwlError> {
        Ok(Self(bytes.try_into().map_err(|_| MwlError::Truncated {
            offset: 0,
            needed: Self::ENCODED_LEN,
        })?))
    }

    #[must_use]
    pub const fn level_number(&self) -> u16 {
        u16::from_le_bytes([self.0[0], self.0[1]])
    }

    pub fn set_level_number(&mut self, level: u16) {
        self.0[..2].copy_from_slice(&level.to_le_bytes());
    }
}
