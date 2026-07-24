use crate::{FixedTableEncodingError, table_encoding::checked_table_len};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OverworldEndpoint {
    pub x: u16,
    pub y: u16,
    pub submap: u8,
}

impl OverworldEndpoint {
    pub const ENCODED_LEN: usize = 5;

    /// Decodes the exact packed five-byte endpoint record.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless exactly five bytes are provided.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(bytes.len());
        }
        Ok(Self {
            x: u16::from_le_bytes([bytes[0], bytes[1]]),
            y: u16::from_le_bytes([bytes[2], bytes[3]]),
            submap: bytes[4],
        })
    }

    #[must_use]
    pub fn encode(self) -> [u8; Self::ENCODED_LEN] {
        let mut bytes = [0; Self::ENCODED_LEN];
        bytes[..2].copy_from_slice(&self.x.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.y.to_le_bytes());
        bytes[4] = self.submap;
        bytes
    }

    /// Decodes a packed endpoint table.
    ///
    /// # Errors
    ///
    /// Returns the input length if it is not a whole number of records.
    pub fn decode_all(bytes: &[u8]) -> Result<Vec<Self>, usize> {
        if bytes.len() % Self::ENCODED_LEN != 0 {
            return Err(bytes.len());
        }
        bytes
            .chunks_exact(Self::ENCODED_LEN)
            .map(Self::decode)
            .collect()
    }

    /// Encodes a complete endpoint table after exact aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`FixedTableEncodingError`] when five bytes per endpoint overflow.
    pub fn encode_all(endpoints: &[Self]) -> Result<Vec<u8>, FixedTableEncodingError> {
        let mut encoded =
            Vec::with_capacity(checked_table_len(endpoints.len(), Self::ENCODED_LEN)?);
        for endpoint in endpoints {
            encoded.extend_from_slice(&endpoint.encode());
        }
        Ok(encoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_table_round_trips() {
        let endpoints = [
            OverworldEndpoint {
                x: 1,
                y: 2,
                submap: 3,
            },
            OverworldEndpoint {
                x: 0x1234,
                y: 0xabcd,
                submap: 6,
            },
        ];
        let bytes = OverworldEndpoint::encode_all(&endpoints).unwrap();
        assert_eq!(OverworldEndpoint::decode_all(&bytes).unwrap(), endpoints);
    }
}
