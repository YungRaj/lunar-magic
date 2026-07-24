use crate::{CodecError, decode_sized_rle_prefix, encode_sized_rle};

/// One Lunar Magic pair of back-to-back sized-RLE streams.
///
/// Lunar Magic stores the even bytes of a table first and the odd bytes second.
/// Each plane has exactly half of `bytes.len()` decoded bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedInterleavedRle {
    pub bytes: Vec<u8>,
    pub first_stream_len: usize,
    pub consumed: usize,
}

/// Decodes two back-to-back sized-RLE byte planes and interleaves them.
///
/// # Errors
///
/// Returns [`CodecError`] when `decoded_len` is odd or either stream is malformed.
pub fn decode_interleaved_sized_rle_prefix(
    input: &[u8],
    decoded_len: usize,
) -> Result<DecodedInterleavedRle, CodecError> {
    if decoded_len % 2 != 0 {
        return Err(CodecError::InvalidDecodedLength(decoded_len));
    }
    let plane_len = decoded_len / 2;
    let even = decode_sized_rle_prefix(input, plane_len)?;
    let odd = decode_sized_rle_prefix(&input[even.consumed..], plane_len)?;
    let mut bytes = Vec::with_capacity(decoded_len);
    for (&even, &odd) in even.bytes.iter().zip(&odd.bytes) {
        bytes.push(even);
        bytes.push(odd);
    }
    Ok(DecodedInterleavedRle {
        bytes,
        first_stream_len: even.consumed,
        consumed: even.consumed + odd.consumed,
    })
}

/// Encodes a byte table as Lunar Magic's even-plane/odd-plane sized-RLE pair.
///
/// # Errors
///
/// Returns [`CodecError`] when the source length is odd.
pub fn encode_interleaved_sized_rle(input: &[u8]) -> Result<Vec<u8>, CodecError> {
    if input.len() % 2 != 0 {
        return Err(CodecError::InvalidDecodedLength(input.len()));
    }
    let mut even = Vec::with_capacity(input.len() / 2);
    let mut odd = Vec::with_capacity(input.len() / 2);
    for pair in input.chunks_exact(2) {
        even.push(pair[0]);
        odd.push(pair[1]);
    }
    let mut encoded = encode_sized_rle(&even);
    encoded.extend_from_slice(&encode_sized_rle(&odd));
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_plane_boundary_and_interleaving() {
        let source = [1, 9, 1, 8, 1, 7, 2, 6, 2, 5, 2, 4];
        let encoded = encode_interleaved_sized_rle(&source).unwrap();
        let decoded = decode_interleaved_sized_rle_prefix(&encoded, source.len()).unwrap();
        assert_eq!(decoded.bytes, source);
        assert_eq!(decoded.consumed, encoded.len());
        assert!(decoded.first_stream_len < decoded.consumed);
    }

    #[test]
    fn odd_table_lengths_are_rejected() {
        assert!(matches!(
            encode_interleaved_sized_rle(&[1, 2, 3]),
            Err(CodecError::InvalidDecodedLength(3))
        ));
        assert!(matches!(
            decode_interleaved_sized_rle_prefix(&[], 3),
            Err(CodecError::InvalidDecodedLength(3))
        ));
    }
}
