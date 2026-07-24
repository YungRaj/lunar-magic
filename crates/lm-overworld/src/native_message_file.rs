//! Bounded lossless exchange format for expanded native overworld messages.

use crate::OverworldMessage;

const MAGIC: &[u8; 8] = b"LMOWMSG1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverworldMessageFileError {
    TooShort,
    Magic,
    InvalidCount(usize),
    Length { actual: usize, expected: usize },
}

impl std::fmt::Display for OverworldMessageFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "invalid native overworld-message file: {self:?}")
    }
}

impl std::error::Error for OverworldMessageFileError {}

/// Encodes an even 194–512-message table with exact fixed-size editor records.
///
/// # Errors
///
/// Rejects counts outside Lunar Magic's expanded multi-bank representation.
pub fn encode_native_overworld_message_file(
    messages: &[OverworldMessage],
) -> Result<Vec<u8>, OverworldMessageFileError> {
    validate_count(messages.len())?;
    let mut output = Vec::with_capacity(10 + messages.len() * OverworldMessage::ENCODED_LEN);
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(
        &u16::try_from(messages.len())
            .map_err(|_| OverworldMessageFileError::InvalidCount(messages.len()))?
            .to_le_bytes(),
    );
    output.extend(
        OverworldMessage::encode_all(messages)
            .map_err(|_| OverworldMessageFileError::InvalidCount(messages.len()))?,
    );
    Ok(output)
}

/// Decodes one complete `LMOWMSG1` artifact.
///
/// # Errors
///
/// Rejects malformed framing, invalid counts, truncation, and trailing bytes.
pub fn decode_native_overworld_message_file(
    bytes: &[u8],
) -> Result<Vec<OverworldMessage>, OverworldMessageFileError> {
    if bytes.len() < 10 {
        return Err(OverworldMessageFileError::TooShort);
    }
    if &bytes[..8] != MAGIC {
        return Err(OverworldMessageFileError::Magic);
    }
    let count = usize::from(u16::from_le_bytes([bytes[8], bytes[9]]));
    validate_count(count)?;
    let expected = 10 + count * OverworldMessage::ENCODED_LEN;
    if bytes.len() != expected {
        return Err(OverworldMessageFileError::Length {
            actual: bytes.len(),
            expected,
        });
    }
    OverworldMessage::decode_all(&bytes[10..]).map_err(|_| OverworldMessageFileError::Length {
        actual: bytes.len(),
        expected,
    })
}

fn validate_count(count: usize) -> Result<(), OverworldMessageFileError> {
    if !(194..=512).contains(&count) || count % 2 != 0 {
        return Err(OverworldMessageFileError::InvalidCount(count));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_round_trip_and_all_framing_failures() {
        let messages = vec![OverworldMessage([0x1f; 144]); 200];
        let encoded = encode_native_overworld_message_file(&messages).unwrap();
        assert_eq!(
            decode_native_overworld_message_file(&encoded).unwrap(),
            messages
        );
        for end in 0..encoded.len() {
            assert!(decode_native_overworld_message_file(&encoded[..end]).is_err());
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode_native_overworld_message_file(&trailing).is_err());
    }
}
