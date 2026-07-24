use std::ops::Range;

pub const HEADER_LEN: usize = 8;
pub const SIGNATURE: &[u8; 4] = b"STAR";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatsBlock {
    pub header_offset: usize,
    pub payload: Range<usize>,
}

impl RatsBlock {
    #[must_use]
    pub fn full_range(&self) -> Range<usize> {
        self.header_offset..self.payload.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeaderError {
    Truncated,
    Signature,
    Complement,
    PayloadOutOfBounds,
}

/// Parses and validates one RATS header and its bounded payload.
///
/// # Errors
///
/// Returns [`HeaderError`] when the header is truncated, malformed, or points outside `bytes`.
pub fn parse_at(bytes: &[u8], offset: usize) -> Result<RatsBlock, HeaderError> {
    let header_end = offset
        .checked_add(HEADER_LEN)
        .ok_or(HeaderError::Truncated)?;
    let header = bytes
        .get(offset..header_end)
        .ok_or(HeaderError::Truncated)?;
    if &header[..4] != SIGNATURE {
        return Err(HeaderError::Signature);
    }
    let size_minus_one = u16::from_le_bytes([header[4], header[5]]);
    let complement = u16::from_le_bytes([header[6], header[7]]);
    if size_minus_one ^ complement != 0xffff {
        return Err(HeaderError::Complement);
    }
    let start = header_end;
    let end = start
        .checked_add(usize::from(size_minus_one) + 1)
        .ok_or(HeaderError::PayloadOutOfBounds)?;
    if end > bytes.len() {
        return Err(HeaderError::PayloadOutOfBounds);
    }
    Ok(RatsBlock {
        header_offset: offset,
        payload: start..end,
    })
}

#[must_use]
pub fn make_header(payload_len: usize) -> Option<[u8; HEADER_LEN]> {
    let size_minus_one = u16::try_from(payload_len.checked_sub(1)?).ok()?;
    let complement = !size_minus_one;
    let mut result = [0; HEADER_LEN];
    result[..4].copy_from_slice(SIGNATURE);
    result[4..6].copy_from_slice(&size_minus_one.to_le_bytes());
    result[6..8].copy_from_slice(&complement.to_le_bytes());
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_header() {
        let mut bytes = make_header(3).unwrap().to_vec();
        bytes.extend_from_slice(&[1, 2, 3]);
        assert_eq!(parse_at(&bytes, 0).unwrap().payload, 8..11);
    }

    #[test]
    fn rejects_bad_complement() {
        let mut bytes = make_header(3).unwrap().to_vec();
        bytes[6] ^= 1;
        bytes.extend_from_slice(&[1, 2, 3]);
        assert_eq!(parse_at(&bytes, 0), Err(HeaderError::Complement));
    }
}
