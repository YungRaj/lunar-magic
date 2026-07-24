use crate::{CodecError, ensure_room};

/// One decoded RLE stream and its exact encoded extent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedRle {
    pub bytes: Vec<u8>,
    pub consumed: usize,
}

/// Decodes the terminated byte-run representation with an output bound.
///
/// # Errors
///
/// Returns [`CodecError`] for truncation, a missing terminator, or output overflow.
pub fn decode_terminated_rle(input: &[u8], output_limit: usize) -> Result<Vec<u8>, CodecError> {
    let decoded = decode_terminated_rle_prefix(input, output_limit)?;
    if decoded.consumed != input.len() {
        return Err(CodecError::TrailingCompressedData(
            input.len() - decoded.consumed,
        ));
    }
    Ok(decoded.bytes)
}

/// Decodes one terminated byte-run stream and reports its exact extent through `FF FF`.
///
/// # Errors
///
/// Returns [`CodecError`] for truncation, a missing terminator, or output overflow.
pub fn decode_terminated_rle_prefix(
    input: &[u8],
    output_limit: usize,
) -> Result<DecodedRle, CodecError> {
    decode_rle(input, output_limit, true)
}

/// Decodes byte-run data until exactly `expected_len` bytes are produced.
///
/// # Errors
///
/// Returns [`CodecError`] for truncation or output overflow.
pub fn decode_sized_rle(input: &[u8], expected_len: usize) -> Result<Vec<u8>, CodecError> {
    let decoded = decode_sized_rle_prefix(input, expected_len)?;
    if decoded.consumed != input.len() {
        return Err(CodecError::TrailingCompressedData(
            input.len() - decoded.consumed,
        ));
    }
    Ok(decoded.bytes)
}

/// Decodes sized byte-run packets and reports how many encoded bytes produced `expected_len`.
///
/// # Errors
///
/// Returns [`CodecError`] for truncation or a packet crossing the declared output length.
pub fn decode_sized_rle_prefix(
    input: &[u8],
    expected_len: usize,
) -> Result<DecodedRle, CodecError> {
    decode_rle(input, expected_len, false)
}

fn decode_rle(
    input: &[u8],
    output_limit: usize,
    terminated: bool,
) -> Result<DecodedRle, CodecError> {
    let mut cursor = 0;
    let mut output = Vec::new();
    loop {
        if !terminated && output.len() == output_limit {
            return Ok(DecodedRle {
                bytes: output,
                consumed: cursor,
            });
        }
        let control = *input.get(cursor).ok_or(if terminated {
            CodecError::MissingTerminator
        } else {
            CodecError::UnexpectedEnd
        })?;
        cursor += 1;
        if terminated && control == 0xff && input.get(cursor) == Some(&0xff) {
            return Ok(DecodedRle {
                bytes: output,
                consumed: cursor + 1,
            });
        }
        let len = usize::from(control & 0x7f) + 1;
        ensure_room(&output, len, output_limit)?;
        if control & 0x80 == 0 {
            let end = cursor.checked_add(len).ok_or(CodecError::UnexpectedEnd)?;
            output.extend_from_slice(input.get(cursor..end).ok_or(CodecError::UnexpectedEnd)?);
            cursor = end;
        } else {
            let value = *input.get(cursor).ok_or(CodecError::UnexpectedEnd)?;
            cursor += 1;
            output.resize(output.len() + len, value);
        }
    }
}

#[must_use]
pub fn encode_terminated_rle(input: &[u8]) -> Vec<u8> {
    let mut output = encode_rle_packets(input, true);
    output.extend_from_slice(&[0xff, 0xff]);
    output
}

/// Encodes byte-run packets without a terminator for a container that supplies decoded length.
#[must_use]
pub fn encode_sized_rle(input: &[u8]) -> Vec<u8> {
    encode_rle_packets(input, false)
}

fn encode_rle_packets(input: &[u8], avoid_terminator_collision: bool) -> Vec<u8> {
    let mut output = Vec::new();
    let mut cursor = 0;
    while cursor < input.len() {
        let mut run = repeated_len(&input[cursor..]).min(128);
        if avoid_terminator_collision && run == 128 && input[cursor] == 0xff {
            run = 127;
        }
        if run >= 3 {
            output.push(0x80 | u8::try_from(run - 1).unwrap_or(127));
            output.push(input[cursor]);
            cursor += run;
            continue;
        }
        let start = cursor;
        cursor += run;
        while cursor < input.len() && cursor - start < 128 {
            let next = repeated_len(&input[cursor..]).min(128);
            if next >= 3 || cursor - start + next > 128 {
                break;
            }
            cursor += next;
        }
        output.push(u8::try_from(cursor - start - 1).unwrap_or(127));
        output.extend_from_slice(&input[start..cursor]);
    }
    output
}

fn repeated_len(input: &[u8]) -> usize {
    input.iter().take_while(|byte| **byte == input[0]).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminated_round_trip() {
        let source = b"abcdddddddddefghiiii";
        let encoded = encode_terminated_rle(source);
        assert_eq!(decode_terminated_rle(&encoded, 100).unwrap(), source);
    }

    #[test]
    fn sized_round_trip_has_no_terminator_and_handles_packet_edges() {
        let mut source = vec![3; 130];
        source.extend(0_u8..=127);
        source.extend_from_slice(&[9, 9]);
        let encoded = encode_sized_rle(&source);
        assert_ne!(
            encoded.get(encoded.len().saturating_sub(2)..),
            Some(&[0xff, 0xff][..])
        );
        assert_eq!(decode_sized_rle(&encoded, source.len()).unwrap(), source);
        assert!(encode_sized_rle(&[]).is_empty());
        assert_eq!(decode_sized_rle(&[], 0).unwrap(), []);
    }

    #[test]
    fn sized_decoder_rejects_packets_that_cross_declared_length() {
        assert!(matches!(
            decode_sized_rle(&[0x82, 7], 2),
            Err(CodecError::OutputLimitExceeded { .. })
        ));
    }

    #[test]
    fn prefix_decoders_report_exact_extent_and_sized_api_rejects_trailing_data() {
        let terminated = [0x02, 1, 2, 3, 0xff, 0xff, 9, 8];
        let decoded = decode_terminated_rle_prefix(&terminated, 3).unwrap();
        assert_eq!(decoded.bytes, [1, 2, 3]);
        assert_eq!(decoded.consumed, 6);
        assert_eq!(
            decode_terminated_rle(&terminated, 3),
            Err(CodecError::TrailingCompressedData(2))
        );

        let sized = [0x82, 7, 0xaa, 0xbb];
        let decoded = decode_sized_rle_prefix(&sized, 3).unwrap();
        assert_eq!(decoded.bytes, [7, 7, 7]);
        assert_eq!(decoded.consumed, 2);
        assert_eq!(
            decode_sized_rle(&sized, 3),
            Err(CodecError::TrailingCompressedData(2))
        );
        assert_eq!(
            decode_sized_rle(&[0], 0),
            Err(CodecError::TrailingCompressedData(1))
        );
    }

    #[test]
    fn every_small_packet_boundary_round_trips_and_every_truncation_fails() {
        for len in 0_usize..=300 {
            let source: Vec<_> = (0..len)
                .map(|index| {
                    if index % 17 < 6 {
                        0xff
                    } else {
                        index.to_le_bytes()[0]
                    }
                })
                .collect();
            let sized = encode_sized_rle(&source);
            assert_eq!(decode_sized_rle(&sized, len).unwrap(), source);
            for end in 0..sized.len() {
                assert!(decode_sized_rle(&sized[..end], len).is_err());
            }
            let terminated = encode_terminated_rle(&source);
            assert_eq!(decode_terminated_rle(&terminated, len).unwrap(), source);
            for end in 0..terminated.len() {
                assert!(decode_terminated_rle(&terminated[..end], len).is_err());
            }
        }
    }

    #[test]
    fn exhaustive_short_sources_round_trip_at_the_exact_output_limit() {
        for len in 0_u32..=9 {
            let cases = 3_usize.pow(len);
            for mut value in 0..cases {
                let mut source = vec![0; usize::try_from(len).unwrap()];
                for byte in &mut source {
                    *byte = u8::try_from(value % 3).unwrap();
                    value /= 3;
                }
                let terminated = encode_terminated_rle(&source);
                assert_eq!(
                    decode_terminated_rle(&terminated, source.len()).unwrap(),
                    source
                );
                let sized = encode_sized_rle(&source);
                assert_eq!(decode_sized_rle(&sized, source.len()).unwrap(), source);
                if !source.is_empty() {
                    let limit = source.len() - 1;
                    assert!(decode_terminated_rle(&terminated, limit).is_err());
                    // A sized stream can fail either on the crossing packet or as trailing data
                    // when a packet ends exactly at the deliberately short declared length.
                    assert!(decode_sized_rle(&sized, limit).is_err());
                }
            }
        }
    }

    #[test]
    fn terminated_encoder_never_emits_an_ambiguous_ff_ff_data_packet() {
        for len in [128, 129, 255, 256, 257, 1024] {
            let source = vec![0xff; len];
            let encoded = encode_terminated_rle(&source);
            assert_eq!(decode_terminated_rle(&encoded, len).unwrap(), source);
            assert_eq!(&encoded[encoded.len() - 2..], [0xff, 0xff]);
            let mut cursor = 0;
            while encoded[cursor..] != [0xff, 0xff] {
                let control = encoded[cursor];
                cursor += 1;
                let packet_len = usize::from(control & 0x7f) + 1;
                if control & 0x80 == 0 {
                    cursor += packet_len;
                } else {
                    assert_ne!((control, encoded[cursor]), (0xff, 0xff));
                    cursor += 1;
                }
            }
        }

        let sized = encode_sized_rle(&[0xff; 128]);
        assert_eq!(sized, [0xff, 0xff]);
        assert_eq!(decode_sized_rle(&sized, 128).unwrap(), vec![0xff; 128]);
    }
}
