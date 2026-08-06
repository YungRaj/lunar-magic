use crate::{CodecError, ensure_room};

mod encode;

pub use encode::{encode_lz2, encode_lz2_literals};

/// A decoded `LC_LZ2` stream and the exact number of source bytes through its terminator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedLz2 {
    pub bytes: Vec<u8>,
    pub consumed: usize,
}

/// Decodes all valid `LC_LZ2` commands used by Lunar Magic.
///
/// # Errors
///
/// Returns [`CodecError`] for truncated, malformed, unsupported, or oversized streams.
pub fn decode_lz2(input: &[u8], output_limit: usize) -> Result<Vec<u8>, CodecError> {
    let decoded = decode_lz2_prefix(input, output_limit)?;
    if decoded.consumed != input.len() {
        return Err(CodecError::TrailingCompressedData(
            input.len() - decoded.consumed,
        ));
    }
    Ok(decoded.bytes)
}

/// Decodes one `LC_LZ2` stream and reports its exact encoded extent.
///
/// Bytes following the `0xff` terminator are deliberately left unconsumed. This is useful for
/// native ROM data that lives in an untagged, bank-bounded region and therefore has no separate
/// compressed-length field.
///
/// # Errors
///
/// Returns [`CodecError`] for truncated, malformed, reserved-command, or oversized streams.
pub fn decode_lz2_prefix(input: &[u8], output_limit: usize) -> Result<DecodedLz2, CodecError> {
    let mut cursor = 0;
    let mut output = Vec::new();
    loop {
        let header = *input.get(cursor).ok_or(CodecError::MissingTerminator)?;
        cursor += 1;
        if header == 0xff {
            return Ok(DecodedLz2 {
                bytes: output,
                consumed: cursor,
            });
        }
        let (command, len) = decode_header(header, input, &mut cursor)?;
        ensure_room(&output, len, output_limit)?;
        match command {
            0 => copy_literal(input, &mut cursor, len, &mut output)?,
            1 => fill_byte(input, &mut cursor, len, &mut output)?,
            2 => fill_word(input, &mut cursor, len, &mut output)?,
            3 => fill_incrementing(input, &mut cursor, len, &mut output)?,
            4..=7 => copy_dictionary(input, &mut cursor, len, &mut output)?,
            other => return Err(CodecError::UnsupportedLz2Command(other)),
        }
    }
}

fn decode_header(header: u8, input: &[u8], cursor: &mut usize) -> Result<(u8, usize), CodecError> {
    if header & 0xe0 == 0xe0 {
        let next = *input.get(*cursor).ok_or(CodecError::UnexpectedEnd)?;
        *cursor += 1;
        Ok((
            (header >> 2) & 7,
            ((((header & 3) as usize) << 8) | usize::from(next)) + 1,
        ))
    } else {
        Ok((header >> 5, usize::from(header & 0x1f) + 1))
    }
}

fn copy_literal(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
    output: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let end = cursor.checked_add(len).ok_or(CodecError::UnexpectedEnd)?;
    output.extend_from_slice(input.get(*cursor..end).ok_or(CodecError::UnexpectedEnd)?);
    *cursor = end;
    Ok(())
}

fn fill_byte(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
    output: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let value = *input.get(*cursor).ok_or(CodecError::UnexpectedEnd)?;
    *cursor += 1;
    output.resize(output.len() + len, value);
    Ok(())
}

fn fill_word(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
    output: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let first = *input.get(*cursor).ok_or(CodecError::UnexpectedEnd)?;
    let second = *input.get(*cursor + 1).ok_or(CodecError::UnexpectedEnd)?;
    *cursor += 2;
    output.extend((0..len).map(|index| if index & 1 == 0 { first } else { second }));
    Ok(())
}

fn fill_incrementing(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
    output: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let start = *input.get(*cursor).ok_or(CodecError::UnexpectedEnd)?;
    *cursor += 1;
    output.extend((0..len).map(|index| start.wrapping_add(index.to_le_bytes()[0])));
    Ok(())
}

fn copy_dictionary(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
    output: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let high = *input.get(*cursor).ok_or(CodecError::UnexpectedEnd)?;
    let low = *input.get(*cursor + 1).ok_or(CodecError::UnexpectedEnd)?;
    *cursor += 2;
    let source = (usize::from(high) << 8) | usize::from(low);
    for index in 0..len {
        let address = source + index;
        let value = *output
            .get(address)
            .ok_or(CodecError::InvalidBackReference {
                offset: address,
                produced: output.len(),
            })?;
        output.push(value);
    }
    Ok(())
}

#[cfg(test)]
#[path = "lz2_tests.rs"]
mod tests;
