use super::validation::validate_secondary_exit;
use super::{
    MwlSecondaryExit, MwlSecondaryExitDecodeError, SecondaryExit, SecondaryExitEncodingError,
};

impl MwlSecondaryExit {
    pub const ENCODED_LEN: usize = 8;

    /// Decodes one MWL packed exit and retargets it to the imported level.
    ///
    /// # Errors
    ///
    /// Rejects a wrong record shape or a target outside the native nine-bit level namespace.
    pub fn decode(bytes: &[u8], target_level: u16) -> Result<Self, MwlSecondaryExitDecodeError> {
        if bytes.len() != Self::ENCODED_LEN {
            return Err(MwlSecondaryExitDecodeError::WrongLength(bytes.len()));
        }
        if target_level > 0x01ff {
            return Err(MwlSecondaryExitDecodeError::TargetLevelOutOfRange(
                target_level,
            ));
        }
        Ok(Self {
            index: u16::from_le_bytes([bytes[0], bytes[1]]),
            exit: SecondaryExit {
                destination_level: target_level,
                position_and_method: bytes[2],
                screen: bytes[3] & 0x1f,
                y: bytes[3] >> 5,
                x: bytes[5] & 0x0f,
                destination_flags: bytes[4] & !8,
                x_and_overworld_flags: bytes[5] & 0xf0,
                additional_flags: bytes[6],
            },
            reserved: bytes[7],
        })
    }

    /// Encodes one packed MWL exit without masking unrepresentable fields.
    ///
    /// # Errors
    ///
    /// Rejects any secondary-exit field that cannot be represented by its native bit field.
    pub fn encode(self) -> Result<[u8; Self::ENCODED_LEN], SecondaryExitEncodingError> {
        validate_secondary_exit(&self.exit, 0)?;
        let index = self.index.to_le_bytes();
        Ok([
            index[0],
            index[1],
            self.exit.position_and_method,
            self.exit.y << 5 | self.exit.screen,
            self.exit.destination_flags
                | if self.exit.destination_level & 0x100 != 0 {
                    8
                } else {
                    0
                },
            self.exit.x_and_overworld_flags | self.exit.x,
            self.exit.additional_flags,
            self.reserved,
        ])
    }

    /// Decodes a complete packed MWL exit section.
    ///
    /// # Errors
    ///
    /// Rejects partial records and targets outside the native level namespace.
    pub fn decode_all(
        bytes: &[u8],
        target_level: u16,
    ) -> Result<Vec<Self>, MwlSecondaryExitDecodeError> {
        if target_level > 0x01ff {
            return Err(MwlSecondaryExitDecodeError::TargetLevelOutOfRange(
                target_level,
            ));
        }
        if bytes.len() % Self::ENCODED_LEN != 0 {
            return Err(MwlSecondaryExitDecodeError::WrongLength(bytes.len()));
        }
        bytes
            .chunks_exact(Self::ENCODED_LEN)
            .map(|record| Self::decode(record, target_level))
            .collect()
    }

    /// Encodes complete packed MWL records without normalizing native fields.
    ///
    /// # Errors
    ///
    /// Reports aggregate-size overflow or the index of an unrepresentable record.
    pub fn encode_all(records: &[Self]) -> Result<Vec<u8>, SecondaryExitEncodingError> {
        let mut bytes = Vec::with_capacity(mwl_secondary_exit_encoded_len(records.len())?);
        for (entry, record) in records.iter().copied().enumerate() {
            validate_secondary_exit(&record.exit, entry)?;
            bytes.extend_from_slice(&record.encode()?);
        }
        Ok(bytes)
    }
}

pub(super) fn mwl_secondary_exit_encoded_len(
    records: usize,
) -> Result<usize, SecondaryExitEncodingError> {
    records
        .checked_mul(MwlSecondaryExit::ENCODED_LEN)
        .ok_or(SecondaryExitEncodingError::SizeOverflow { records })
}
