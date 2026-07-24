//! Portable, bounded ownership evidence for explicit RATS reclamation.

use crate::RatsOwnershipManifest;
use lm_rats::RatsBlock;
use std::fmt;

const MAGIC: &[u8; 8] = b"LMRATS01";
const HEADER_LEN: usize = 16;
const ENTRY_LEN: usize = 24;
const MAX_BLOCKS: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatsOwnershipManifestFile(pub RatsOwnershipManifest);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RatsManifestFileError {
    TooLarge(usize),
    WrongMagic,
    Truncated,
    TrailingBytes(usize),
    CountLimit {
        actual: usize,
        maximum: usize,
    },
    OffsetOverflow,
    InvalidDescriptor {
        collection: &'static str,
        index: usize,
    },
    DuplicateDescriptor {
        collection: &'static str,
        index: usize,
    },
    RetainedBlockNotOwned {
        index: usize,
    },
}

impl fmt::Display for RatsManifestFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid RATS ownership manifest: {self:?}")
    }
}

impl std::error::Error for RatsManifestFileError {}

impl RatsOwnershipManifestFile {
    pub const MAX_FILE_LEN: usize = HEADER_LEN + ENTRY_LEN * MAX_BLOCKS * 2;

    /// Encodes the canonical `LMRATS01` representation.
    ///
    /// # Errors
    ///
    /// Rejects excessive counts, invalid ranges, duplicate entries, retained entries outside the
    /// owned set, or offsets that cannot be represented by the portable 64-bit format.
    pub fn encode(&self) -> Result<Vec<u8>, RatsManifestFileError> {
        validate_manifest(&self.0)?;
        let owned =
            u32::try_from(self.0.owned.len()).map_err(|_| count_error(self.0.owned.len()))?;
        let retained =
            u32::try_from(self.0.retained.len()).map_err(|_| count_error(self.0.retained.len()))?;
        let mut output = Vec::with_capacity(
            HEADER_LEN + ENTRY_LEN * (self.0.owned.len() + self.0.retained.len()),
        );
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&owned.to_le_bytes());
        output.extend_from_slice(&retained.to_le_bytes());
        let mut owned_blocks = self.0.owned.clone();
        let mut retained_blocks = self.0.retained.clone();
        owned_blocks.sort_by_key(|block| block.header_offset);
        retained_blocks.sort_by_key(|block| block.header_offset);
        encode_blocks(&mut output, &owned_blocks)?;
        encode_blocks(&mut output, &retained_blocks)?;
        Ok(output)
    }

    /// Decodes one complete bounded `LMRATS01` artifact.
    ///
    /// # Errors
    ///
    /// Rejects wrong framing, excessive counts, arithmetic overflow, malformed descriptors,
    /// duplicates, trailing bytes, or a retained entry not present in the owned set.
    pub fn decode(bytes: &[u8]) -> Result<Self, RatsManifestFileError> {
        if bytes.len() > Self::MAX_FILE_LEN {
            return Err(RatsManifestFileError::TooLarge(bytes.len()));
        }
        let header = bytes
            .get(..HEADER_LEN)
            .ok_or(RatsManifestFileError::Truncated)?;
        if &header[..8] != MAGIC {
            return Err(RatsManifestFileError::WrongMagic);
        }
        let owned_count = read_count(&header[8..12])?;
        let retained_count = read_count(&header[12..16])?;
        validate_count(owned_count)?;
        validate_count(retained_count)?;
        let total = owned_count
            .checked_add(retained_count)
            .ok_or(RatsManifestFileError::OffsetOverflow)?;
        let expected = HEADER_LEN
            .checked_add(
                total
                    .checked_mul(ENTRY_LEN)
                    .ok_or(RatsManifestFileError::OffsetOverflow)?,
            )
            .ok_or(RatsManifestFileError::OffsetOverflow)?;
        if bytes.len() < expected {
            return Err(RatsManifestFileError::Truncated);
        }
        if bytes.len() > expected {
            return Err(RatsManifestFileError::TrailingBytes(bytes.len() - expected));
        }
        let mut cursor = HEADER_LEN;
        let owned = decode_blocks(bytes, &mut cursor, owned_count)?;
        let retained = decode_blocks(bytes, &mut cursor, retained_count)?;
        let manifest = RatsOwnershipManifest { owned, retained };
        validate_manifest(&manifest)?;
        Ok(Self(manifest))
    }
}

fn encode_blocks(output: &mut Vec<u8>, blocks: &[RatsBlock]) -> Result<(), RatsManifestFileError> {
    for block in blocks {
        for value in [block.header_offset, block.payload.start, block.payload.end] {
            output.extend_from_slice(
                &u64::try_from(value)
                    .map_err(|_| RatsManifestFileError::OffsetOverflow)?
                    .to_le_bytes(),
            );
        }
    }
    Ok(())
}

fn decode_blocks(
    bytes: &[u8],
    cursor: &mut usize,
    count: usize,
) -> Result<Vec<RatsBlock>, RatsManifestFileError> {
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let entry = bytes
            .get(*cursor..*cursor + ENTRY_LEN)
            .ok_or(RatsManifestFileError::Truncated)?;
        *cursor += ENTRY_LEN;
        blocks.push(RatsBlock {
            header_offset: read_offset(&entry[..8])?,
            payload: read_offset(&entry[8..16])?..read_offset(&entry[16..24])?,
        });
    }
    Ok(blocks)
}

fn validate_manifest(manifest: &RatsOwnershipManifest) -> Result<(), RatsManifestFileError> {
    validate_collection("owned", &manifest.owned)?;
    validate_collection("retained", &manifest.retained)?;
    for (index, block) in manifest.retained.iter().enumerate() {
        if !manifest.owned.contains(block) {
            return Err(RatsManifestFileError::RetainedBlockNotOwned { index });
        }
    }
    Ok(())
}

fn validate_collection(
    collection: &'static str,
    blocks: &[RatsBlock],
) -> Result<(), RatsManifestFileError> {
    validate_count(blocks.len())?;
    for (index, block) in blocks.iter().enumerate() {
        if block.payload.start != block.header_offset.saturating_add(8)
            || block.payload.start >= block.payload.end
        {
            return Err(RatsManifestFileError::InvalidDescriptor { collection, index });
        }
        if blocks[..index].contains(block) {
            return Err(RatsManifestFileError::DuplicateDescriptor { collection, index });
        }
    }
    Ok(())
}

fn read_count(bytes: &[u8]) -> Result<usize, RatsManifestFileError> {
    usize::try_from(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| RatsManifestFileError::Truncated)?,
    ))
    .map_err(|_| RatsManifestFileError::OffsetOverflow)
}

fn read_offset(bytes: &[u8]) -> Result<usize, RatsManifestFileError> {
    usize::try_from(u64::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| RatsManifestFileError::Truncated)?,
    ))
    .map_err(|_| RatsManifestFileError::OffsetOverflow)
}

fn validate_count(count: usize) -> Result<(), RatsManifestFileError> {
    if count > MAX_BLOCKS {
        Err(count_error(count))
    } else {
        Ok(())
    }
}

const fn count_error(actual: usize) -> RatsManifestFileError {
    RatsManifestFileError::CountLimit {
        actual,
        maximum: MAX_BLOCKS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(offset: usize, len: usize) -> RatsBlock {
        RatsBlock {
            header_offset: offset,
            payload: offset + 8..offset + 8 + len,
        }
    }

    #[test]
    fn canonical_round_trip_preserves_exact_descriptors() {
        let expected = RatsOwnershipManifestFile(RatsOwnershipManifest {
            owned: vec![block(0x100, 3), block(0x200, 7)],
            retained: vec![block(0x200, 7)],
        });
        let bytes = expected.encode().unwrap();
        assert_eq!(RatsOwnershipManifestFile::decode(&bytes).unwrap(), expected);
        assert_eq!(
            RatsOwnershipManifestFile::decode(&bytes)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn canonical_encoding_orders_both_collections_by_header_offset() {
        let file = RatsOwnershipManifestFile(RatsOwnershipManifest {
            owned: vec![block(0x200, 7), block(0x100, 3)],
            retained: vec![block(0x200, 7), block(0x100, 3)],
        });
        let decoded = RatsOwnershipManifestFile::decode(&file.encode().unwrap()).unwrap();
        assert_eq!(decoded.0.owned, vec![block(0x100, 3), block(0x200, 7)]);
        assert_eq!(decoded.0.retained, decoded.0.owned);
    }

    #[test]
    fn every_truncated_prefix_and_trailing_data_is_rejected() {
        let bytes = RatsOwnershipManifestFile(RatsOwnershipManifest {
            owned: vec![block(0x100, 3)],
            retained: Vec::new(),
        })
        .encode()
        .unwrap();
        for end in 0..bytes.len() {
            assert!(RatsOwnershipManifestFile::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            RatsOwnershipManifestFile::decode(&trailing),
            Err(RatsManifestFileError::TrailingBytes(1))
        );
    }

    #[test]
    fn invalid_duplicate_and_foreign_retained_descriptors_are_rejected() {
        for manifest in [
            RatsOwnershipManifest {
                owned: vec![RatsBlock {
                    header_offset: 0x100,
                    payload: 0x109..0x10a,
                }],
                retained: Vec::new(),
            },
            RatsOwnershipManifest {
                owned: vec![block(0x100, 1), block(0x100, 1)],
                retained: Vec::new(),
            },
            RatsOwnershipManifest {
                owned: vec![block(0x100, 1)],
                retained: vec![block(0x200, 1)],
            },
        ] {
            assert!(RatsOwnershipManifestFile(manifest).encode().is_err());
        }
    }

    #[test]
    fn oversized_declared_count_is_rejected_before_allocation() {
        let mut bytes = Vec::from(MAGIC);
        bytes.extend_from_slice(&(u32::try_from(MAX_BLOCKS).unwrap() + 1).to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        assert!(matches!(
            RatsOwnershipManifestFile::decode(&bytes),
            Err(RatsManifestFileError::CountLimit { .. })
        ));
    }
}
