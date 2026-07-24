use crate::{CompactExAnimation, ExAnimationError};
use std::fmt;

/// A versioned wrapper around the canonical compact ROM representation of one `ExAnimation` slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactExAnimationFile {
    pub source_slot: u16,
    pub animation: CompactExAnimation,
}

impl CompactExAnimationFile {
    pub const MAGIC: [u8; 8] = *b"LMEXAN1\0";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 16;
    pub const MAX_PAYLOAD_LEN: usize = 0x1_0000;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN + Self::MAX_PAYLOAD_LEN;

    /// Encodes the animation using the caller's recovered 256-entry size-mode table.
    ///
    /// # Errors
    ///
    /// Returns [`CompactExAnimationFileError`] for invalid animation data or oversized output.
    pub fn encode(
        &self,
        double_size_modes: &[bool],
    ) -> Result<Vec<u8>, CompactExAnimationFileError> {
        validate_modes(double_size_modes)?;
        let payload = self.animation.encode(double_size_modes)?;
        validate_payload_len(payload.len())?;
        let payload_len =
            u32::try_from(payload.len()).map_err(|_| CompactExAnimationFileError::Overflow)?;
        let mut bytes = Vec::with_capacity(Self::HEADER_LEN + payload.len());
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.source_slot.to_le_bytes());
        bytes.extend_from_slice(&payload_len.to_le_bytes());
        bytes.extend_from_slice(&payload);
        Ok(bytes)
    }

    /// Decodes a compact animation, requiring the payload parser to consume every byte.
    ///
    /// # Errors
    ///
    /// Returns [`CompactExAnimationFileError`] for framing, size-mode, compact-record, length, or
    /// trailing-data errors.
    pub fn decode(
        bytes: &[u8],
        maximum_records: usize,
        double_size_modes: &[bool],
    ) -> Result<Self, CompactExAnimationFileError> {
        validate_modes(double_size_modes)?;
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(CompactExAnimationFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(CompactExAnimationFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(CompactExAnimationFileError::UnsupportedVersion(version));
        }
        let source_slot = u16::from_le_bytes([header[10], header[11]]);
        let payload_len = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| CompactExAnimationFileError::Overflow)?;
        validate_payload_len(payload_len)?;
        let expected = Self::HEADER_LEN
            .checked_add(payload_len)
            .ok_or(CompactExAnimationFileError::Overflow)?;
        if bytes.len() != expected {
            return Err(CompactExAnimationFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let payload = &bytes[Self::HEADER_LEN..];
        let (animation, consumed) =
            CompactExAnimation::decode(payload, maximum_records, double_size_modes)?;
        if consumed != payload.len() {
            return Err(CompactExAnimationFileError::UnconsumedPayload {
                consumed,
                actual: payload.len(),
            });
        }
        Ok(Self {
            source_slot,
            animation,
        })
    }
}

fn validate_modes(double_size_modes: &[bool]) -> Result<(), CompactExAnimationFileError> {
    if double_size_modes.len() == 256 {
        Ok(())
    } else {
        Err(CompactExAnimationFileError::WrongSizeModeCount(
            double_size_modes.len(),
        ))
    }
}

fn validate_payload_len(len: usize) -> Result<(), CompactExAnimationFileError> {
    if len > CompactExAnimationFile::MAX_PAYLOAD_LEN {
        Err(CompactExAnimationFileError::PayloadTooLarge(len))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub enum CompactExAnimationFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    WrongSizeModeCount(usize),
    PayloadTooLarge(usize),
    WrongLength { expected: usize, actual: usize },
    UnconsumedPayload { consumed: usize, actual: usize },
    Overflow,
    Animation(ExAnimationError),
}

impl fmt::Display for CompactExAnimationFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid compact ExAnimation file: {self:?}")
    }
}

impl std::error::Error for CompactExAnimationFileError {}

impl From<ExAnimationError> for CompactExAnimationFileError {
    fn from(value: ExAnimationError) -> Self {
        Self::Animation(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExAnimationRecord;

    fn file() -> CompactExAnimationFile {
        let mut trigger_values = [0; 16];
        trigger_values[3] = 7;
        CompactExAnimationFile {
            source_slot: 0x105,
            animation: CompactExAnimation {
                setting: 2,
                header_value: 0x1234_5678,
                trigger_mask: 1 << 3,
                trigger_values,
                records: vec![
                    ExAnimationRecord::new(1, 1, 4, 0x321, true, &[1, 2, 3, 4], false).unwrap(),
                ],
            },
        }
    }

    #[test]
    fn compact_animation_round_trips_with_explicit_modes() {
        let modes = [false; 256];
        let file = file();
        assert_eq!(
            CompactExAnimationFile::decode(&file.encode(&modes).unwrap(), 32, &modes).unwrap(),
            file
        );
    }

    #[test]
    fn mode_count_version_and_trailing_bytes_are_rejected() {
        let modes = [false; 256];
        assert!(matches!(
            file().encode(&modes[..255]),
            Err(CompactExAnimationFileError::WrongSizeModeCount(255))
        ));
        let bytes = file().encode(&modes).unwrap();
        let mut version = bytes.clone();
        version[8..10].copy_from_slice(&2_u16.to_le_bytes());
        assert!(matches!(
            CompactExAnimationFile::decode(&version, 32, &modes),
            Err(CompactExAnimationFileError::UnsupportedVersion(2))
        ));
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            CompactExAnimationFile::decode(&trailing, 32, &modes),
            Err(CompactExAnimationFileError::WrongLength { .. })
        ));
    }

    #[test]
    fn declared_payload_must_be_consumed_exactly() {
        let modes = [false; 256];
        let mut bytes = file().encode(&modes).unwrap();
        let payload_len = u32::try_from(bytes.len() - CompactExAnimationFile::HEADER_LEN).unwrap();
        bytes[12..16].copy_from_slice(&(payload_len + 1).to_le_bytes());
        bytes.push(0xaa);
        assert!(matches!(
            CompactExAnimationFile::decode(&bytes, 32, &modes),
            Err(CompactExAnimationFileError::UnconsumedPayload { .. })
        ));
    }
}
