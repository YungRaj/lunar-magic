//! Portable lossless container for one native overworld warp-link table.

use crate::{OverworldWarpLinkTable, OverworldWarpLinkTableError};

const MAGIC: &[u8; 8] = b"LMOWWR1\0";
const HEADER_LEN: usize = 12;
const RECORD_LEN: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldWarpLinkFileError {
    WrongMagic,
    Truncated,
    TrailingBytes,
    Reserved([u8; 2]),
    LengthOverflow,
    Table(OverworldWarpLinkTableError),
}

impl std::fmt::Display for OverworldWarpLinkFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native overworld warp-link file: {self:?}"
        )
    }
}

impl std::error::Error for OverworldWarpLinkFileError {}

impl From<OverworldWarpLinkTableError> for OverworldWarpLinkFileError {
    fn from(value: OverworldWarpLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl OverworldWarpLinkTable {
    /// Encodes an interleaved lossless native warp-link exchange file.
    ///
    /// # Errors
    ///
    /// Returns a table error for excessive records or size overflow.
    pub fn encode_native_warp_file(&self) -> Result<Vec<u8>, OverworldWarpLinkFileError> {
        let planes = self.encode_planes()?;
        let body_len = self
            .links
            .len()
            .checked_mul(RECORD_LEN)
            .ok_or(OverworldWarpLinkFileError::LengthOverflow)?;
        let mut encoded = Vec::with_capacity(
            HEADER_LEN
                .checked_add(body_len)
                .ok_or(OverworldWarpLinkFileError::LengthOverflow)?,
        );
        encoded.extend_from_slice(MAGIC);
        let count = u16::try_from(self.links.len())
            .map_err(|_| OverworldWarpLinkFileError::LengthOverflow)?;
        encoded.extend_from_slice(&count.to_le_bytes());
        encoded.extend_from_slice(&[0, 0]);
        for index in 0..self.links.len() {
            let offset = index * 2;
            encoded.extend_from_slice(&planes.source_vertical[offset..offset + 2]);
            encoded.extend_from_slice(&planes.source_horizontal[offset..offset + 2]);
            encoded.extend_from_slice(&planes.destination_vertical[offset..offset + 2]);
            encoded.extend_from_slice(&planes.destination_horizontal[offset..offset + 2]);
        }
        Ok(encoded)
    }

    /// Decodes one complete native warp-link exchange file.
    ///
    /// # Errors
    ///
    /// Rejects bad framing, reserved bytes, overflow, trailing data, and invalid tables.
    pub fn decode_native_warp_file(bytes: &[u8]) -> Result<Self, OverworldWarpLinkFileError> {
        if bytes.len() < HEADER_LEN {
            return Err(OverworldWarpLinkFileError::Truncated);
        }
        if &bytes[..8] != MAGIC {
            return Err(OverworldWarpLinkFileError::WrongMagic);
        }
        let reserved = [bytes[10], bytes[11]];
        if reserved != [0, 0] {
            return Err(OverworldWarpLinkFileError::Reserved(reserved));
        }
        let count = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        let expected = count
            .checked_mul(RECORD_LEN)
            .and_then(|body| HEADER_LEN.checked_add(body))
            .ok_or(OverworldWarpLinkFileError::LengthOverflow)?;
        if bytes.len() < expected {
            return Err(OverworldWarpLinkFileError::Truncated);
        }
        if bytes.len() > expected {
            return Err(OverworldWarpLinkFileError::TrailingBytes);
        }
        let mut planes = [
            Vec::with_capacity(count * 2),
            Vec::with_capacity(count * 2),
            Vec::with_capacity(count * 2),
            Vec::with_capacity(count * 2),
        ];
        for record in bytes[HEADER_LEN..].chunks_exact(RECORD_LEN) {
            for (index, plane) in planes.iter_mut().enumerate() {
                let offset = index * 2;
                plane.extend_from_slice(&record[offset..offset + 2]);
            }
        }
        Ok(Self::decode_planes(
            &planes[0], &planes[1], &planes[2], &planes[3],
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OverworldWarpEndpoint, OverworldWarpLink};

    #[test]
    fn canonical_file_round_trips_and_requires_exact_consumption() {
        let table = OverworldWarpLinkTable {
            links: vec![OverworldWarpLink {
                source: OverworldWarpEndpoint {
                    packed_vertical: 0x0210,
                    horizontal_tile: 7,
                },
                destination: OverworldWarpEndpoint {
                    packed_vertical: 0x04a8,
                    horizontal_tile: 18,
                },
            }],
        };
        let encoded = table.encode_native_warp_file().unwrap();
        assert_eq!(
            OverworldWarpLinkTable::decode_native_warp_file(&encoded).unwrap(),
            table
        );
        for end in 0..encoded.len() {
            assert!(OverworldWarpLinkTable::decode_native_warp_file(&encoded[..end]).is_err());
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            OverworldWarpLinkTable::decode_native_warp_file(&trailing),
            Err(OverworldWarpLinkFileError::TrailingBytes)
        );
    }
}
