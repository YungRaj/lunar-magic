use super::{SecondaryExitEncodingError, SecondaryExitTable};

const MAGIC: &[u8; 8] = b"LMSEXIT1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecondaryExitTableFileError {
    Length { actual: usize, expected: usize },
    Magic,
    TableLength(usize),
    Table(SecondaryExitEncodingError),
}

impl std::fmt::Display for SecondaryExitTableFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid secondary-exit table file: {self:?}")
    }
}

impl std::error::Error for SecondaryExitTableFileError {}

impl From<SecondaryExitEncodingError> for SecondaryExitTableFileError {
    fn from(value: SecondaryExitEncodingError) -> Self {
        Self::Table(value)
    }
}

impl SecondaryExitTable {
    pub const FILE_LEN: usize = MAGIC.len() + Self::ENTRY_COUNT * Self::PLANE_COUNT;

    /// Encodes all six native planes with an allocation-independent header.
    ///
    /// # Errors
    ///
    /// Rejects a table that cannot be represented by the native six-plane encoding.
    pub fn encode_native_file(&self) -> Result<Vec<u8>, SecondaryExitTableFileError> {
        let payload = self.encode()?;
        let mut output = Vec::with_capacity(Self::FILE_LEN);
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&payload);
        Ok(output)
    }

    /// Decodes one exact `LMSEXIT1` table.
    ///
    /// # Errors
    ///
    /// Rejects wrong framing or a payload other than six complete 8,192-byte planes.
    pub fn decode_native_file(bytes: &[u8]) -> Result<Self, SecondaryExitTableFileError> {
        if bytes.len() != Self::FILE_LEN {
            return Err(SecondaryExitTableFileError::Length {
                actual: bytes.len(),
                expected: Self::FILE_LEN,
            });
        }
        if &bytes[..MAGIC.len()] != MAGIC {
            return Err(SecondaryExitTableFileError::Magic);
        }
        Self::decode(&bytes[MAGIC.len()..]).map_err(SecondaryExitTableFileError::TableLength)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SecondaryExit;

    #[test]
    fn native_file_round_trips_every_plane() {
        let mut entries = vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT];
        entries[0x123] = SecondaryExit {
            destination_level: 0x1ab,
            position_and_method: 0x35,
            screen: 7,
            x: 4,
            y: 8,
            destination_flags: 0x61,
            x_and_overworld_flags: 0xd0,
            additional_flags: 0xa5,
        };
        let table = SecondaryExitTable { entries };
        assert_eq!(
            SecondaryExitTable::decode_native_file(&table.encode_native_file().unwrap()).unwrap(),
            table
        );
    }
}
