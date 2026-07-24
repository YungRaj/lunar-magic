use crate::BossSequenceMessage;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BossSequenceMessageTable {
    pub messages: [BossSequenceMessage; Self::MESSAGE_COUNT],
}

impl Default for BossSequenceMessageTable {
    fn default() -> Self {
        Self {
            messages: std::array::from_fn(|_| {
                BossSequenceMessage([Self::BLANK_GLYPH; BossSequenceMessage::ENCODED_LEN])
            }),
        }
    }
}

impl BossSequenceMessageTable {
    pub const MESSAGE_COUNT: usize = 7;
    pub const ROW_COUNT: usize = Self::MESSAGE_COUNT * BossSequenceMessage::ROWS;
    pub const ROW_GLYPHS: usize = BossSequenceMessage::COLUMNS;
    pub const INTERLEAVED_ROW_LEN: usize = Self::ROW_GLYPHS * 2;
    pub const NATIVE_ROW_LEN: usize = 4 + Self::INTERLEAVED_ROW_LEN + 1;
    pub const NATIVE_PAYLOAD_LEN: usize = Self::ROW_COUNT * Self::NATIVE_ROW_LEN;
    pub const BLANK_GLYPH: u8 = 0x1f;
    pub const ATTRIBUTE_BYTE: u8 = 0x39;

    /// Encodes Lunar Magic's 56 fixed native row records.
    #[must_use]
    pub fn encode_native_payload(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(Self::NATIVE_PAYLOAD_LEN);
        for (row_index, row) in self
            .messages
            .iter()
            .flat_map(|message| message.encoded().chunks_exact(Self::ROW_GLYPHS))
            .enumerate()
        {
            let tilemap_address =
                0x5344_u16.wrapping_sub(u16::try_from(row_index & 7).unwrap_or(0) * 0x20);
            output.extend_from_slice(&tilemap_address.to_be_bytes());
            output.extend_from_slice(&[0x00, 0x2f]);
            for glyph in row {
                output.extend_from_slice(&[*glyph, Self::ATTRIBUTE_BYTE]);
            }
            output.push(0xff);
        }
        output
    }

    /// Decodes 56 complete native row records.
    ///
    /// # Errors
    ///
    /// Rejects the wrong aggregate length, altered row headers/terminators, or malformed message
    /// reconstruction.
    pub fn decode_native_payload(bytes: &[u8]) -> Result<Self, BossSequenceTableError> {
        if bytes.len() != Self::NATIVE_PAYLOAD_LEN {
            return Err(BossSequenceTableError::PayloadLength(bytes.len()));
        }
        let mut glyphs = Vec::with_capacity(Self::MESSAGE_COUNT * BossSequenceMessage::ENCODED_LEN);
        for (row_index, row) in bytes.chunks_exact(Self::NATIVE_ROW_LEN).enumerate() {
            let expected_address =
                0x5344_u16.wrapping_sub(u16::try_from(row_index & 7).unwrap_or(0) * 0x20);
            if row[..4]
                != [
                    expected_address.to_be_bytes()[0],
                    expected_address.to_be_bytes()[1],
                    0x00,
                    0x2f,
                ]
                || row[Self::NATIVE_ROW_LEN - 1] != 0xff
            {
                return Err(BossSequenceTableError::RowFraming(row_index));
            }
            for pair in row[4..4 + Self::INTERLEAVED_ROW_LEN].chunks_exact(2) {
                if pair[1] != Self::ATTRIBUTE_BYTE {
                    return Err(BossSequenceTableError::Attribute {
                        row: row_index,
                        column: glyphs.len() % Self::ROW_GLYPHS,
                        actual: pair[1],
                    });
                }
                glyphs.push(pair[0]);
            }
        }
        let mut chunks = glyphs.chunks_exact(BossSequenceMessage::ENCODED_LEN);
        let messages = std::array::from_fn(|_| {
            BossSequenceMessage::decode(chunks.next().unwrap_or(&[]))
                .unwrap_or(BossSequenceMessage([Self::BLANK_GLYPH; 192]))
        });
        Ok(Self { messages })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BossSequenceTableError {
    PayloadLength(usize),
    RowFraming(usize),
    Attribute {
        row: usize,
        column: usize,
        actual: u8,
    },
}

impl std::fmt::Display for BossSequenceTableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid boss-sequence table: {self:?}")
    }
}

impl std::error::Error for BossSequenceTableError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_rows_round_trip_with_exact_native_headers_and_attributes() {
        let mut table = BossSequenceMessageTable::default();
        table.messages[6].0[191] = 0xab;
        let encoded = table.encode_native_payload();
        assert_eq!(encoded.len(), BossSequenceMessageTable::NATIVE_PAYLOAD_LEN);
        assert_eq!(
            BossSequenceMessageTable::decode_native_payload(&encoded).unwrap(),
            table
        );
        assert_eq!(&encoded[..4], &[0x53, 0x44, 0x00, 0x2f]);
        assert_eq!(
            &encoded[7 * BossSequenceMessageTable::NATIVE_ROW_LEN..][..4],
            &[0x52, 0x64, 0x00, 0x2f]
        );
    }
}
