use super::*;

#[test]
fn decodes_every_recovered_command_and_both_reference_forms() {
    let stream = [
        0x03, 1, 2, 4, 8, // literal
        0x22, 0xaa, // byte fill
        0x43, 0x10, 0x20, // word fill
        0x62, // zero fill
        0x83, 0x00, 0x00, // absolute forward
        0xa3, 0x83, // relative bit-reversed forward
        0xc3, 0x00, 0x03, // absolute reverse from index 3
        0xfc, 0x03, 0x83, // extended command 7, relative reverse
        0xff,
    ];
    assert_eq!(
        decode_lz3(&stream, 64).unwrap(),
        [
            1, 2, 4, 8, 0xaa, 0xaa, 0xaa, 0x10, 0x20, 0x10, 0x20, 0, 0, 0, 1, 2, 4, 8, 0x80, 0x40,
            0x20, 0x10, 8, 4, 2, 1, 8, 0x10, 0x20, 0x40,
        ]
    );
}

#[test]
fn deterministic_encoder_round_trips_and_uses_lz3_fills() {
    let mut source = vec![0; 80];
    source.extend(vec![7; 40]);
    source.extend([1, 2].into_iter().cycle().take(41));
    source.extend_from_slice(b"literal tail");
    let encoded = encode_lz3(&source);
    assert_eq!(decode_lz3(&encoded, source.len()).unwrap(), source);
    assert!(encoded.len() < source.len());
    assert!(encoded.iter().any(|byte| byte >> 5 == 3));
}

#[test]
fn deterministic_encoder_uses_every_dictionary_transform() {
    for (expected, source) in [
        (4, b"abcdWXYZabcdWXYZ".to_vec()),
        (
            5,
            [0x01_u8, 0x23, 0x45, 0x67, 0x80, 0xc4, 0xa2, 0xe6].to_vec(),
        ),
        (6, b"abcdWXYZZYXWdcba".to_vec()),
    ] {
        let encoded = encode_lz3(&source);
        assert_eq!(decode_lz3(&encoded, source.len()).unwrap(), source);
        assert!(commands(&encoded).contains(&expected), "{encoded:02x?}");
    }
}

#[test]
fn encoder_selects_relative_and_absolute_dictionary_operands() {
    let relative = encode_lz3(b"abcdefghabcdefgh");
    assert!(
        dictionary_operands(&relative)
            .iter()
            .any(|(command, operand)| *command == 4
                && operand.len() == 1
                && operand[0] & 0x80 != 0)
    );

    let mut source = b"ABCD".to_vec();
    source.extend((0_u8..200).map(|index| 0x80 | index & 0x3f));
    source.extend_from_slice(b"ABCD");
    let absolute = encode_lz3(&source);
    assert_eq!(decode_lz3(&absolute, source.len()).unwrap(), source);
    assert!(
        dictionary_operands(&absolute)
            .iter()
            .any(|(command, operand)| *command == 4 && operand.as_slice() == [0, 0])
    );
}

#[test]
fn encoder_uses_relative_dictionary_sources_above_absolute_address_space() {
    let mut state = 0x1234_5678_u32;
    let mut source = (0..0x8040)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        })
        .collect::<Vec<_>>();
    let repeated = source[source.len() - 64..].to_vec();
    source.extend_from_slice(&repeated);

    let encoded = encode_lz3(&source);
    assert_eq!(decode_lz3(&encoded, source.len()).unwrap(), source);
    assert!(has_relative_dictionary_after(&encoded, 0x8000));
}

#[test]
fn overlapping_forward_match_compresses_repetition() {
    let source = b"abcabcabcabcabcabcabcabc";
    let encoded = encode_lz3(source);
    assert_eq!(decode_lz3(&encoded, source.len()).unwrap(), source);
    assert!(commands(&encoded).contains(&4));
    assert!(encoded.len() < source.len());
}

#[test]
fn exhaustive_short_sources_round_trip_at_the_exact_limit() {
    for len in 0_u32..=9 {
        let cases = 3_usize.pow(len);
        for mut value in 0..cases {
            let mut source = vec![0; usize::try_from(len).unwrap()];
            for byte in &mut source {
                *byte = u8::try_from(value % 3).unwrap();
                value /= 3;
            }
            let encoded = encode_lz3(&source);
            assert_eq!(decode_lz3(&encoded, source.len()).unwrap(), source);
            if !source.is_empty() {
                assert_eq!(
                    decode_lz3(&encoded, source.len() - 1),
                    Err(CodecError::OutputLimitExceeded {
                        limit: source.len() - 1
                    })
                );
            }
        }
    }
}

#[test]
fn every_two_byte_input_is_a_total_prefix_decode_operation() {
    for first in 0_u8..=u8::MAX {
        for second in 0_u8..=u8::MAX {
            let _ = decode_lz3_prefix(&[first, second], 2048);
        }
    }
}

#[test]
fn prefix_extent_trailing_data_and_limits_are_exact() {
    let stream = [0x00, 7, 0xff, 0xaa];
    assert_eq!(
        decode_lz3_prefix(&stream, 1).unwrap(),
        DecodedLz3 {
            bytes: vec![7],
            consumed: 3,
        }
    );
    assert_eq!(
        decode_lz3(&stream, 1),
        Err(CodecError::TrailingCompressedData(1))
    );
    assert_eq!(
        decode_lz3(&[0x61, 0xff], 1),
        Err(CodecError::OutputLimitExceeded { limit: 1 })
    );
}

#[test]
fn malformed_references_and_truncations_fail() {
    assert!(matches!(
        decode_lz3(&[0x80, 0x80, 0xff], 8),
        Err(CodecError::InvalidBackReference { .. })
    ));
    assert!(matches!(
        decode_lz3(&[0xc1, 0x00, 0x00, 0xff], 8),
        Err(CodecError::InvalidBackReference { .. })
    ));
    assert_eq!(decode_lz3(&[0xe0], 8), Err(CodecError::UnexpectedEnd));
    assert_eq!(decode_lz3(&[], 8), Err(CodecError::MissingTerminator));
}

fn commands(encoded: &[u8]) -> Vec<u8> {
    dictionary_operands(encoded)
        .into_iter()
        .map(|(command, _)| command)
        .collect()
}

fn dictionary_operands(encoded: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut cursor = 0;
    let mut result = Vec::new();
    while encoded[cursor] != 0xff {
        let header = encoded[cursor];
        cursor += 1;
        let (command, len) = if header & 0xe0 == 0xe0 {
            let next = encoded[cursor];
            cursor += 1;
            (
                (header >> 2) & 7,
                ((((header & 3) as usize) << 8) | usize::from(next)) + 1,
            )
        } else {
            (header >> 5, usize::from(header & 0x1f) + 1)
        };
        let operand_len = match command {
            0 => len,
            1 => 1,
            3 => 0,
            4..=7 if encoded[cursor] & 0x80 != 0 => 1,
            2 | 4..=7 => 2,
            _ => unreachable!(),
        };
        let operand = encoded[cursor..cursor + operand_len].to_vec();
        if command >= 4 {
            result.push((command, operand));
        } else {
            result.push((command, Vec::new()));
        }
        cursor += operand_len;
    }
    result
}

fn has_relative_dictionary_after(encoded: &[u8], threshold: usize) -> bool {
    let mut cursor = 0;
    let mut produced = 0;
    while encoded[cursor] != 0xff {
        let header = encoded[cursor];
        cursor += 1;
        let (command, len) = if header & 0xe0 == 0xe0 {
            let next = encoded[cursor];
            cursor += 1;
            (
                (header >> 2) & 7,
                ((((header & 3) as usize) << 8) | usize::from(next)) + 1,
            )
        } else {
            (header >> 5, usize::from(header & 0x1f) + 1)
        };
        let relative = matches!(command, 4..=7) && encoded[cursor] & 0x80 != 0;
        let operand_len = match command {
            0 => len,
            1 => 1,
            3 => 0,
            4..=7 if relative => 1,
            2 | 4..=7 => 2,
            _ => unreachable!(),
        };
        if produced > threshold && relative {
            return true;
        }
        cursor += operand_len;
        produced += len;
    }
    false
}
