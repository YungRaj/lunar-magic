use crate::{FixedTableEncodingError, table_encoding::checked_table_len};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverworldMessage(pub [u8; Self::ENCODED_LEN]);

impl OverworldMessage {
    pub const ROWS: usize = 8;
    pub const COLUMNS: usize = 18;
    pub const ENCODED_LEN: usize = Self::ROWS * Self::COLUMNS;

    #[must_use]
    pub fn row(&self, row: usize) -> Option<&[u8]> {
        let start = row.checked_mul(Self::COLUMNS)?;
        self.0.get(start..start + Self::COLUMNS)
    }

    /// Decodes one exact-size tilemap.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless it is exactly 144 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        Ok(Self(bytes.try_into().map_err(|_| bytes.len())?))
    }

    #[must_use]
    pub const fn encoded(&self) -> &[u8; Self::ENCODED_LEN] {
        &self.0
    }

    /// Decodes a contiguous sequence of exact-size message records.
    ///
    /// # Errors
    ///
    /// Returns the supplied length when it is not message-aligned.
    pub fn decode_all(bytes: &[u8]) -> Result<Vec<Self>, usize> {
        if bytes.len() % Self::ENCODED_LEN != 0 {
            return Err(bytes.len());
        }
        bytes
            .chunks_exact(Self::ENCODED_LEN)
            .map(Self::decode)
            .collect()
    }

    /// Encodes complete fixed-size message tilemaps after aggregate-size preflight.
    ///
    /// # Errors
    ///
    /// Returns [`FixedTableEncodingError`] when 144 bytes per message overflow.
    pub fn encode_all(messages: &[Self]) -> Result<Vec<u8>, FixedTableEncodingError> {
        let mut encoded = Vec::with_capacity(checked_table_len(messages.len(), Self::ENCODED_LEN)?);
        for message in messages {
            encoded.extend_from_slice(&message.0);
        }
        Ok(encoded)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BossSequenceMessage(pub [u8; Self::ENCODED_LEN]);

impl BossSequenceMessage {
    pub const ROWS: usize = 8;
    pub const COLUMNS: usize = 24;
    pub const ENCODED_LEN: usize = Self::ROWS * Self::COLUMNS;

    /// Decodes one exact-size boss-sequence tilemap.
    ///
    /// # Errors
    ///
    /// Returns the supplied length unless it is exactly 192 bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, usize> {
        Ok(Self(bytes.try_into().map_err(|_| bytes.len())?))
    }

    #[must_use]
    pub const fn encoded(&self) -> &[u8; Self::ENCODED_LEN] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VanillaOverworldMessageError {
    MappingLength(usize),
    PointerLength(usize),
    PointerOutsideText { message: usize, offset: usize },
    TruncatedRow { message: usize, row: usize },
}

impl std::fmt::Display for VanillaOverworldMessageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid vanilla overworld-message tables: {self:?}"
        )
    }
}

impl std::error::Error for VanillaOverworldMessageError {}

/// Materializes SMW's 97×2 logical message slots from its 23-entry selector map, 25-entry
/// relative pointer table, and row-terminated text blob.
///
/// The original renderer treats the high bit on a glyph as "blank the remainder of this
/// 18-column row". This decoder removes that storage encoding while retaining the glyph's low
/// seven bits in the fixed 8×18 editor workspace.
///
/// # Errors
///
/// Rejects wrong fixed-table shapes and any pointer or row that escapes the supplied text blob.
pub fn decode_vanilla_overworld_messages(
    mapping: &[u8],
    pointers: &[u8],
    text: &[u8],
) -> Result<Vec<OverworldMessage>, VanillaOverworldMessageError> {
    const SELECTOR_COUNT: usize = 23;
    const SOURCE_MESSAGE_COUNT: usize = 25;
    const TRANSLEVEL_COUNT: usize = 97;
    if mapping.len() != SELECTOR_COUNT {
        return Err(VanillaOverworldMessageError::MappingLength(mapping.len()));
    }
    if pointers.len() != SOURCE_MESSAGE_COUNT * 2 {
        return Err(VanillaOverworldMessageError::PointerLength(pointers.len()));
    }
    let offsets: Vec<_> = pointers
        .chunks_exact(2)
        .map(|word| usize::from(u16::from_le_bytes([word[0], word[1]])))
        .collect();
    let mut output = Vec::with_capacity(TRANSLEVEL_COUNT * 2);
    for translevel in 0..TRANSLEVEL_COUNT {
        for trigger in 1..=2 {
            let source = select_vanilla_message(mapping, translevel, trigger);
            output.push(decode_vanilla_message(text, offsets[source], source)?);
        }
    }
    Ok(output)
}

fn select_vanilla_message(mapping: &[u8], translevel: usize, trigger: usize) -> usize {
    (1..mapping.len())
        .rev()
        .find(|&index| {
            let selector = mapping[index];
            usize::from(selector & 0x7f) == translevel
                && if selector & 0x80 == 0 {
                    trigger == 1
                } else {
                    trigger == 2
                }
        })
        .unwrap_or(0)
}

fn decode_vanilla_message(
    text: &[u8],
    offset: usize,
    message: usize,
) -> Result<OverworldMessage, VanillaOverworldMessageError> {
    if offset >= text.len() {
        return Err(VanillaOverworldMessageError::PointerOutsideText { message, offset });
    }
    let mut source = offset;
    let mut decoded = [0x1f; OverworldMessage::ENCODED_LEN];
    for row in 0..OverworldMessage::ROWS {
        let destination = row * OverworldMessage::COLUMNS;
        let mut terminated = false;
        for column in 0..OverworldMessage::COLUMNS {
            if terminated {
                continue;
            }
            let byte = *text
                .get(source)
                .ok_or(VanillaOverworldMessageError::TruncatedRow { message, row })?;
            source += 1;
            decoded[destination + column] = byte & 0x7f;
            terminated = byte & 0x80 != 0;
        }
    }
    Ok(OverworldMessage(decoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_shapes_are_lossless() {
        let bytes: Vec<_> = (0..OverworldMessage::ENCODED_LEN)
            .map(|index| index.to_le_bytes()[0])
            .collect();
        let message = OverworldMessage::decode(&bytes).unwrap();
        assert_eq!(message.encoded(), bytes.as_slice());
        assert_eq!(message.row(1).unwrap().len(), OverworldMessage::COLUMNS);
        assert!(message.row(OverworldMessage::ROWS).is_none());
        assert_eq!(
            OverworldMessage::decode_all(
                &OverworldMessage::encode_all(std::slice::from_ref(&message)).unwrap(),
            )
            .unwrap(),
            [message]
        );

        let boss = vec![0x55; BossSequenceMessage::ENCODED_LEN];
        assert_eq!(
            BossSequenceMessage::decode(&boss).unwrap().encoded(),
            boss.as_slice()
        );
    }

    #[test]
    fn vanilla_rows_expand_high_bit_termination_and_selector_aliases() {
        let mut mapping = [0; 23];
        mapping[1] = 5;
        mapping[2] = 0x85;
        let mut pointers = [0; 50];
        pointers[2..4].copy_from_slice(&8_u16.to_le_bytes());
        pointers[4..6].copy_from_slice(&16_u16.to_le_bytes());
        let mut text = vec![0x9f; 24];
        text[8..16].fill(0x81);
        text[16..24].fill(0x82);
        let messages = decode_vanilla_overworld_messages(&mapping, &pointers, &text).unwrap();
        assert_eq!(messages.len(), 194);
        assert_eq!(messages[5 * 2].0[0], 1);
        assert_eq!(messages[5 * 2 + 1].0[0], 2);
        assert_eq!(messages[0].0[0], 0x1f);
        assert_eq!(messages[5 * 2].0[1], 0x1f);
    }
}
