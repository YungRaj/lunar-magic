//! Exact portable ownership evidence for ROM-backed palette editing.

use lm_graphics::{PaletteEntryOwner, PaletteOwnership};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaletteOwnershipFile {
    pub ownership: PaletteOwnership,
}

impl PaletteOwnershipFile {
    pub const MAGIC: [u8; 8] = *b"LMPALOWN";
    pub const VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 16;
    pub const RECORD_LEN: usize = 4;
    pub const MAX_COLORS: usize = 65_536;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN + Self::MAX_COLORS * Self::RECORD_LEN;

    /// Encodes one canonical fixed-width ownership record per palette color.
    ///
    /// # Errors
    ///
    /// Returns [`PaletteOwnershipFileError`] if the public ownership map exceeds the file bound.
    pub fn encode(&self) -> Result<Vec<u8>, PaletteOwnershipFileError> {
        let count = self.ownership.len();
        if count > Self::MAX_COLORS {
            return Err(PaletteOwnershipFileError::TooManyColors(count));
        }
        let mut bytes = Vec::with_capacity(encoded_len(count)?);
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&[0; 2]);
        bytes.extend_from_slice(
            &u32::try_from(count)
                .map_err(|_| PaletteOwnershipFileError::Overflow)?
                .to_le_bytes(),
        );
        for index in 0..count {
            let owner = self
                .ownership
                .owner(index)
                .ok_or(PaletteOwnershipFileError::OwnershipShape)?;
            match owner {
                PaletteEntryOwner::Editable => bytes.extend_from_slice(&[0, 0, 0, 0]),
                PaletteEntryOwner::Fixed => bytes.extend_from_slice(&[1, 0, 0, 0]),
                PaletteEntryOwner::ExAnimation { record } => {
                    bytes.extend_from_slice(&[2, 0]);
                    bytes.extend_from_slice(&record.to_le_bytes());
                }
            }
        }
        Ok(bytes)
    }

    /// Decodes one exactly consumed ownership artifact.
    ///
    /// # Errors
    ///
    /// Rejects malformed framing, excessive counts, unknown owner kinds, reserved bytes, and
    /// noncanonical record payloads.
    pub fn decode(bytes: &[u8]) -> Result<Self, PaletteOwnershipFileError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(PaletteOwnershipFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(PaletteOwnershipFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if version != Self::VERSION {
            return Err(PaletteOwnershipFileError::UnsupportedVersion(version));
        }
        if header[10..12] != [0; 2] {
            return Err(PaletteOwnershipFileError::ReservedBytes);
        }
        let count = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| PaletteOwnershipFileError::Overflow)?;
        if count > Self::MAX_COLORS {
            return Err(PaletteOwnershipFileError::TooManyColors(count));
        }
        let expected = encoded_len(count)?;
        if bytes.len() != expected {
            return Err(PaletteOwnershipFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let owners = bytes[Self::HEADER_LEN..]
            .chunks_exact(Self::RECORD_LEN)
            .enumerate()
            .map(|(index, record)| {
                if record[1] != 0 {
                    return Err(PaletteOwnershipFileError::ReservedBytes);
                }
                let payload = u16::from_le_bytes([record[2], record[3]]);
                match (record[0], payload) {
                    (0, 0) => Ok(PaletteEntryOwner::Editable),
                    (1, 0) => Ok(PaletteEntryOwner::Fixed),
                    (2, record) => Ok(PaletteEntryOwner::ExAnimation { record }),
                    (kind @ 0..=1, payload) => {
                        Err(PaletteOwnershipFileError::NonCanonicalPayload {
                            index,
                            kind,
                            payload,
                        })
                    }
                    (kind, _) => Err(PaletteOwnershipFileError::UnknownOwner { index, kind }),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            ownership: PaletteOwnership::from_owners(owners),
        })
    }
}

fn encoded_len(count: usize) -> Result<usize, PaletteOwnershipFileError> {
    count
        .checked_mul(PaletteOwnershipFile::RECORD_LEN)
        .and_then(|records| PaletteOwnershipFile::HEADER_LEN.checked_add(records))
        .ok_or(PaletteOwnershipFileError::Overflow)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteOwnershipFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    ReservedBytes,
    TooManyColors(usize),
    WrongLength {
        expected: usize,
        actual: usize,
    },
    UnknownOwner {
        index: usize,
        kind: u8,
    },
    NonCanonicalPayload {
        index: usize,
        kind: u8,
        payload: u16,
    },
    OwnershipShape,
    Overflow,
}

impl fmt::Display for PaletteOwnershipFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid palette ownership file: {self:?}")
    }
}

impl std::error::Error for PaletteOwnershipFileError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> PaletteOwnershipFile {
        PaletteOwnershipFile {
            ownership: PaletteOwnership::from_owners(vec![
                PaletteEntryOwner::Editable,
                PaletteEntryOwner::Fixed,
                PaletteEntryOwner::ExAnimation { record: 0x1234 },
            ]),
        }
    }

    #[test]
    fn exact_round_trip_preserves_every_owner_kind() {
        let expected = file();
        let bytes = expected.encode().unwrap();
        assert_eq!(PaletteOwnershipFile::decode(&bytes).unwrap(), expected);
        assert_eq!(
            PaletteOwnershipFile::decode(&bytes)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn every_truncation_trailing_byte_and_noncanonical_record_fails() {
        let bytes = file().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(PaletteOwnershipFile::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            PaletteOwnershipFile::decode(&trailing),
            Err(PaletteOwnershipFileError::WrongLength { .. })
        ));
        let mut reserved = bytes.clone();
        reserved[PaletteOwnershipFile::HEADER_LEN + 1] = 1;
        assert_eq!(
            PaletteOwnershipFile::decode(&reserved),
            Err(PaletteOwnershipFileError::ReservedBytes)
        );
        let mut payload = bytes;
        payload[PaletteOwnershipFile::HEADER_LEN + 2..PaletteOwnershipFile::HEADER_LEN + 4]
            .copy_from_slice(&1_u16.to_le_bytes());
        assert!(matches!(
            PaletteOwnershipFile::decode(&payload),
            Err(PaletteOwnershipFileError::NonCanonicalPayload { index: 0, .. })
        ));
    }
}
