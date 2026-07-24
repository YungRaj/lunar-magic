use crate::{CodecError, ensure_room};

mod encode;

pub use encode::encode_lz3;

/// A decoded Lunar Magic LZ3 stream and its exact encoded extent through the terminator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedLz3 {
    pub bytes: Vec<u8>,
    pub consumed: usize,
}

/// Decodes one complete Lunar Magic LZ3 stream.
///
/// LZ3 retains LZ2's compact/extended command headers, but command 3 is a zero fill and dictionary
/// operands support a one-byte backward distance. Extended command 7 is a valid reverse copy.
///
/// # Errors
///
/// Returns [`CodecError`] for truncation, missing termination, invalid dictionary references,
/// trailing data, or output growth beyond `output_limit`.
pub fn decode_lz3(input: &[u8], output_limit: usize) -> Result<Vec<u8>, CodecError> {
    let decoded = decode_lz3_prefix(input, output_limit)?;
    if decoded.consumed != input.len() {
        return Err(CodecError::TrailingCompressedData(
            input.len() - decoded.consumed,
        ));
    }
    Ok(decoded.bytes)
}

/// Decodes the first Lunar Magic LZ3 stream and reports the consumed source extent.
///
/// # Errors
///
/// Returns [`CodecError`] for malformed input or bounded-output violations.
pub fn decode_lz3_prefix(input: &[u8], output_limit: usize) -> Result<DecodedLz3, CodecError> {
    let mut cursor = 0;
    let mut output = Vec::new();
    loop {
        let header = *input.get(cursor).ok_or(CodecError::MissingTerminator)?;
        cursor += 1;
        if header == 0xff {
            return Ok(DecodedLz3 {
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
            3 => output.resize(output.len() + len, 0),
            4 => copy_forward(input, &mut cursor, len, &mut output, false)?,
            5 => copy_forward(input, &mut cursor, len, &mut output, true)?,
            6 | 7 => copy_reverse(input, &mut cursor, len, &mut output)?,
            _ => unreachable!("three-bit command"),
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

fn dictionary_source(
    input: &[u8],
    cursor: &mut usize,
    produced: usize,
) -> Result<usize, CodecError> {
    let first = *input.get(*cursor).ok_or(CodecError::UnexpectedEnd)?;
    *cursor += 1;
    if first & 0x80 != 0 {
        let distance = usize::from(first & 0x7f) + 1;
        produced
            .checked_sub(distance)
            .ok_or(CodecError::InvalidBackReference {
                offset: distance,
                produced,
            })
    } else {
        let second = *input.get(*cursor).ok_or(CodecError::UnexpectedEnd)?;
        *cursor += 1;
        Ok((usize::from(first) << 8) | usize::from(second))
    }
}

fn copy_forward(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
    output: &mut Vec<u8>,
    reverse_bits: bool,
) -> Result<(), CodecError> {
    let source = dictionary_source(input, cursor, output.len())?;
    for index in 0..len {
        let address = source + index;
        let byte = *output
            .get(address)
            .ok_or(CodecError::InvalidBackReference {
                offset: address,
                produced: output.len(),
            })?;
        output.push(if reverse_bits {
            byte.reverse_bits()
        } else {
            byte
        });
    }
    Ok(())
}

fn copy_reverse(
    input: &[u8],
    cursor: &mut usize,
    len: usize,
    output: &mut Vec<u8>,
) -> Result<(), CodecError> {
    let source = dictionary_source(input, cursor, output.len())?;
    for index in 0..len {
        let address = source
            .checked_sub(index)
            .ok_or(CodecError::InvalidBackReference {
                offset: source,
                produced: output.len(),
            })?;
        let byte = *output
            .get(address)
            .ok_or(CodecError::InvalidBackReference {
                offset: address,
                produced: output.len(),
            })?;
        output.push(byte);
    }
    Ok(())
}

#[cfg(test)]
#[path = "lz3_tests.rs"]
mod tests;
