//! Portable lossless container for one native overworld path-link table.

use crate::{OverworldPathLinkTable, OverworldPathLinkTableError};

const MAGIC: &[u8; 8] = b"LMOWLN1\0";
const HEADER_LEN: usize = 12;
const RECORD_LEN: usize = 12;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldPathLinkFileError {
    WrongMagic,
    Truncated,
    TrailingBytes,
    Reserved([u8; 2]),
    LengthOverflow,
    Table(OverworldPathLinkTableError),
}

impl std::fmt::Display for OverworldPathLinkFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid native overworld path-link file: {self:?}"
        )
    }
}

impl std::error::Error for OverworldPathLinkFileError {}

impl From<OverworldPathLinkTableError> for OverworldPathLinkFileError {
    fn from(value: OverworldPathLinkTableError) -> Self {
        Self::Table(value)
    }
}

impl OverworldPathLinkTable {
    /// Encodes a canonical interleaved file for native import/export.
    ///
    /// # Errors
    ///
    /// Returns a table error for excessive records or size overflow.
    pub fn encode_native_file(&self) -> Result<Vec<u8>, OverworldPathLinkFileError> {
        let planes = self.encode_planes()?;
        let body_len = self
            .links
            .len()
            .checked_mul(RECORD_LEN)
            .ok_or(OverworldPathLinkFileError::LengthOverflow)?;
        let mut encoded = Vec::with_capacity(
            HEADER_LEN
                .checked_add(body_len)
                .ok_or(OverworldPathLinkFileError::LengthOverflow)?,
        );
        encoded.extend_from_slice(MAGIC);
        let count = u16::try_from(self.links.len())
            .map_err(|_| OverworldPathLinkFileError::LengthOverflow)?;
        encoded.extend_from_slice(&count.to_le_bytes());
        encoded.extend_from_slice(&[0, 0]);
        for index in 0..self.links.len() {
            let endpoint = index * 5;
            let target = index * 2;
            encoded.extend_from_slice(&planes.sources[endpoint..endpoint + 5]);
            encoded.extend_from_slice(&planes.destinations[endpoint..endpoint + 5]);
            encoded.extend_from_slice(&planes.targets[target..target + 2]);
        }
        Ok(encoded)
    }

    /// Decodes one complete canonical native path-link file.
    ///
    /// # Errors
    ///
    /// Rejects bad framing, reserved bytes, overflow, trailing data, and invalid tables.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, OverworldPathLinkFileError> {
        if bytes.len() < HEADER_LEN {
            return Err(OverworldPathLinkFileError::Truncated);
        }
        if &bytes[..8] != MAGIC {
            return Err(OverworldPathLinkFileError::WrongMagic);
        }
        let reserved = [bytes[10], bytes[11]];
        if reserved != [0, 0] {
            return Err(OverworldPathLinkFileError::Reserved(reserved));
        }
        let count = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        let expected = count
            .checked_mul(RECORD_LEN)
            .and_then(|body| HEADER_LEN.checked_add(body))
            .ok_or(OverworldPathLinkFileError::LengthOverflow)?;
        if bytes.len() < expected {
            return Err(OverworldPathLinkFileError::Truncated);
        }
        if bytes.len() > expected {
            return Err(OverworldPathLinkFileError::TrailingBytes);
        }
        let mut sources = Vec::with_capacity(count * 5);
        let mut destinations = Vec::with_capacity(count * 5);
        let mut targets = Vec::with_capacity(count * 2);
        for record in bytes[HEADER_LEN..].chunks_exact(RECORD_LEN) {
            sources.extend_from_slice(&record[..5]);
            destinations.extend_from_slice(&record[5..10]);
            targets.extend_from_slice(&record[10..12]);
        }
        Ok(Self::decode_planes(&sources, &destinations, &targets)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OverworldEndpoint, OverworldPathLink, OverworldPathTarget};

    fn table() -> OverworldPathLinkTable {
        OverworldPathLinkTable {
            links: vec![OverworldPathLink {
                source: OverworldEndpoint {
                    x: 0x140,
                    y: 0x28,
                    submap: 0,
                },
                destination: OverworldEndpoint {
                    x: 0,
                    y: 0x48,
                    submap: 1,
                },
                target: OverworldPathTarget {
                    y_tile: 0,
                    x_tile: 4,
                },
            }],
        }
    }

    #[test]
    fn canonical_file_round_trips_and_requires_exact_consumption() {
        let encoded = table().encode_native_file().unwrap();
        assert_eq!(
            OverworldPathLinkTable::decode_native_file(&encoded).unwrap(),
            table()
        );
        for end in 0..encoded.len() {
            assert!(OverworldPathLinkTable::decode_native_file(&encoded[..end]).is_err());
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            OverworldPathLinkTable::decode_native_file(&trailing),
            Err(OverworldPathLinkFileError::TrailingBytes)
        );
    }
}
