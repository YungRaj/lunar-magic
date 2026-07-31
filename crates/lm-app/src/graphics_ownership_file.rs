//! Exact portable ownership evidence for ROM-backed graphics editing.

use lm_graphics::{GraphicsOwnership, GraphicsTileOwner};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphicsOwnershipFile {
    pub ownership: GraphicsOwnership,
}

impl GraphicsOwnershipFile {
    pub const MAGIC: [u8; 8] = *b"LMGFXOWN";
    pub const VERSION: u16 = 2;
    pub const LEGACY_VERSION: u16 = 1;
    pub const HEADER_LEN: usize = 16;
    pub const RECORD_LEN: usize = 4;
    pub const MAX_TILES: usize = 65_536;
    pub const MAX_FILE_LEN: usize = Self::HEADER_LEN + Self::MAX_TILES * Self::RECORD_LEN;

    /// Encodes one canonical version-2 fixed-width ownership record per graphics tile.
    ///
    /// # Errors
    ///
    /// Returns [`GraphicsOwnershipFileError`] if the public ownership map exceeds the file bound.
    pub fn encode(&self) -> Result<Vec<u8>, GraphicsOwnershipFileError> {
        let count = self.ownership.len();
        if count > Self::MAX_TILES {
            return Err(GraphicsOwnershipFileError::TooManyTiles(count));
        }
        let mut bytes = Vec::with_capacity(encoded_len(count)?);
        bytes.extend_from_slice(&Self::MAGIC);
        bytes.extend_from_slice(&Self::VERSION.to_le_bytes());
        bytes.extend_from_slice(&[0; 2]);
        bytes.extend_from_slice(
            &u32::try_from(count)
                .map_err(|_| GraphicsOwnershipFileError::Overflow)?
                .to_le_bytes(),
        );
        for index in 0..count {
            let owner = self
                .ownership
                .owner(index)
                .ok_or(GraphicsOwnershipFileError::OwnershipShape)?;
            match owner {
                GraphicsTileOwner::Editable => bytes.extend_from_slice(&[0, 0, 0, 0]),
                GraphicsTileOwner::Fixed => bytes.extend_from_slice(&[1, 0, 0, 0]),
                GraphicsTileOwner::ExAnimation { record } => {
                    bytes.extend_from_slice(&[2, 0]);
                    bytes.extend_from_slice(&record.to_le_bytes());
                }
                GraphicsTileOwner::OriginalAnimation { slot } => {
                    validate_animation_slot(index, 3, slot, 0x7e)?;
                    bytes.extend_from_slice(&[3, 0, slot, 0]);
                }
                GraphicsTileOwner::LevelExAnimation { slot } => {
                    validate_animation_slot(index, 4, slot, 0x3f)?;
                    bytes.extend_from_slice(&[4, 0, slot, 0]);
                }
                GraphicsTileOwner::GlobalExAnimation { slot } => {
                    validate_animation_slot(index, 5, slot, 0x3f)?;
                    bytes.extend_from_slice(&[5, 0, slot, 0]);
                }
            }
        }
        Ok(bytes)
    }

    /// Decodes one exactly consumed version-1 or version-2 graphics ownership artifact.
    ///
    /// # Errors
    ///
    /// Rejects malformed framing, excessive counts, unknown kinds, reserved bytes, and
    /// noncanonical record payloads.
    pub fn decode(bytes: &[u8]) -> Result<Self, GraphicsOwnershipFileError> {
        let header = bytes
            .get(..Self::HEADER_LEN)
            .ok_or(GraphicsOwnershipFileError::Truncated)?;
        if header[..8] != Self::MAGIC {
            return Err(GraphicsOwnershipFileError::WrongMagic);
        }
        let version = u16::from_le_bytes([header[8], header[9]]);
        if ![Self::LEGACY_VERSION, Self::VERSION].contains(&version) {
            return Err(GraphicsOwnershipFileError::UnsupportedVersion(version));
        }
        if header[10..12] != [0; 2] {
            return Err(GraphicsOwnershipFileError::ReservedBytes);
        }
        let count = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .map_err(|_| GraphicsOwnershipFileError::Overflow)?;
        if count > Self::MAX_TILES {
            return Err(GraphicsOwnershipFileError::TooManyTiles(count));
        }
        let expected = encoded_len(count)?;
        if bytes.len() != expected {
            return Err(GraphicsOwnershipFileError::WrongLength {
                expected,
                actual: bytes.len(),
            });
        }
        let owners = bytes[Self::HEADER_LEN..]
            .chunks_exact(Self::RECORD_LEN)
            .enumerate()
            .map(|(index, record)| {
                if record[1] != 0 {
                    return Err(GraphicsOwnershipFileError::ReservedBytes);
                }
                let payload = u16::from_le_bytes([record[2], record[3]]);
                match (record[0], payload) {
                    (0, 0) => Ok(GraphicsTileOwner::Editable),
                    (1, 0) => Ok(GraphicsTileOwner::Fixed),
                    (2, record) => Ok(GraphicsTileOwner::ExAnimation { record }),
                    (3, slot @ 0..=0x7e) if version == Self::VERSION => {
                        Ok(GraphicsTileOwner::OriginalAnimation {
                            slot: u8::try_from(slot).expect("bounded animation slot"),
                        })
                    }
                    (4, slot @ 0..=0x3f) if version == Self::VERSION => {
                        Ok(GraphicsTileOwner::LevelExAnimation {
                            slot: u8::try_from(slot).expect("bounded animation slot"),
                        })
                    }
                    (5, slot @ 0..=0x3f) if version == Self::VERSION => {
                        Ok(GraphicsTileOwner::GlobalExAnimation {
                            slot: u8::try_from(slot).expect("bounded animation slot"),
                        })
                    }
                    (kind @ 3..=5, slot) if version == Self::VERSION => {
                        Err(GraphicsOwnershipFileError::InvalidAnimationSlot { index, kind, slot })
                    }
                    (kind @ 0..=1, payload) => {
                        Err(GraphicsOwnershipFileError::NonCanonicalPayload {
                            index,
                            kind,
                            payload,
                        })
                    }
                    (kind, _) => Err(GraphicsOwnershipFileError::UnknownOwner { index, kind }),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            ownership: GraphicsOwnership::from_owners(owners),
        })
    }
}

fn validate_animation_slot(
    index: usize,
    kind: u8,
    slot: u8,
    maximum: u8,
) -> Result<(), GraphicsOwnershipFileError> {
    if slot > maximum {
        return Err(GraphicsOwnershipFileError::InvalidAnimationSlot {
            index,
            kind,
            slot: u16::from(slot),
        });
    }
    Ok(())
}

fn encoded_len(count: usize) -> Result<usize, GraphicsOwnershipFileError> {
    count
        .checked_mul(GraphicsOwnershipFile::RECORD_LEN)
        .and_then(|records| GraphicsOwnershipFile::HEADER_LEN.checked_add(records))
        .ok_or(GraphicsOwnershipFileError::Overflow)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphicsOwnershipFileError {
    Truncated,
    WrongMagic,
    UnsupportedVersion(u16),
    ReservedBytes,
    TooManyTiles(usize),
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
    InvalidAnimationSlot {
        index: usize,
        kind: u8,
        slot: u16,
    },
    OwnershipShape,
    Overflow,
}

impl fmt::Display for GraphicsOwnershipFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid graphics ownership file: {self:?}")
    }
}

impl std::error::Error for GraphicsOwnershipFileError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> GraphicsOwnershipFile {
        GraphicsOwnershipFile {
            ownership: GraphicsOwnership::from_owners(vec![
                GraphicsTileOwner::Editable,
                GraphicsTileOwner::Fixed,
                GraphicsTileOwner::ExAnimation { record: 0x4321 },
                GraphicsTileOwner::OriginalAnimation { slot: 0x7e },
                GraphicsTileOwner::LevelExAnimation { slot: 0x3f },
                GraphicsTileOwner::GlobalExAnimation { slot: 0x3f },
            ]),
        }
    }

    #[test]
    fn exact_round_trip_preserves_every_owner_kind() {
        let expected = file();
        let bytes = expected.encode().unwrap();
        assert_eq!(GraphicsOwnershipFile::decode(&bytes).unwrap(), expected);
        assert_eq!(
            GraphicsOwnershipFile::decode(&bytes)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn legacy_version_one_decodes_the_original_owner_kinds() {
        let legacy = GraphicsOwnershipFile {
            ownership: GraphicsOwnership::from_owners(vec![
                GraphicsTileOwner::Editable,
                GraphicsTileOwner::Fixed,
                GraphicsTileOwner::ExAnimation { record: 7 },
            ]),
        };
        let mut bytes = legacy.encode().unwrap();
        bytes[8..10].copy_from_slice(&GraphicsOwnershipFile::LEGACY_VERSION.to_le_bytes());
        assert_eq!(GraphicsOwnershipFile::decode(&bytes).unwrap(), legacy);
    }

    #[test]
    fn animation_slot_bounds_are_canonical() {
        for owner in [
            GraphicsTileOwner::OriginalAnimation { slot: 0x7f },
            GraphicsTileOwner::LevelExAnimation { slot: 0x40 },
            GraphicsTileOwner::GlobalExAnimation { slot: 0x40 },
        ] {
            let file = GraphicsOwnershipFile {
                ownership: GraphicsOwnership::from_owners(vec![owner]),
            };
            assert!(matches!(
                file.encode(),
                Err(GraphicsOwnershipFileError::InvalidAnimationSlot { index: 0, .. })
            ));
        }
    }

    #[test]
    fn malformed_framing_and_noncanonical_records_fail() {
        let bytes = file().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(GraphicsOwnershipFile::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            GraphicsOwnershipFile::decode(&trailing),
            Err(GraphicsOwnershipFileError::WrongLength { .. })
        ));
        let mut unknown = bytes.clone();
        unknown[GraphicsOwnershipFile::HEADER_LEN] = 6;
        assert!(matches!(
            GraphicsOwnershipFile::decode(&unknown),
            Err(GraphicsOwnershipFileError::UnknownOwner { index: 0, kind: 6 })
        ));
        let mut legacy_new_kind = file().encode().unwrap();
        legacy_new_kind[8..10]
            .copy_from_slice(&GraphicsOwnershipFile::LEGACY_VERSION.to_le_bytes());
        legacy_new_kind[GraphicsOwnershipFile::HEADER_LEN] = 3;
        assert!(matches!(
            GraphicsOwnershipFile::decode(&legacy_new_kind),
            Err(GraphicsOwnershipFileError::UnknownOwner { index: 0, kind: 3 })
        ));
        let mut payload = bytes;
        payload[GraphicsOwnershipFile::HEADER_LEN + 2..GraphicsOwnershipFile::HEADER_LEN + 4]
            .copy_from_slice(&1_u16.to_le_bytes());
        assert!(matches!(
            GraphicsOwnershipFile::decode(&payload),
            Err(GraphicsOwnershipFileError::NonCanonicalPayload { index: 0, .. })
        ));
    }
}
