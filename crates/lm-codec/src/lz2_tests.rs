use super::*;

#[test]
fn literal_round_trip() {
    let source: Vec<u8> = (0..=255).collect();
    assert_eq!(
        decode_lz2(&encode_lz2_literals(&source), 0x10000).unwrap(),
        source
    );
}

#[test]
fn decodes_copy_variants() {
    let stream = [3, 0x01, 0x02, 0x04, 0x08, 0xa3, 0, 0, 0xc3, 0, 3, 0xff];
    assert_eq!(
        decode_lz2(&stream, 32).unwrap(),
        [1, 2, 4, 8, 128, 64, 32, 16, 8, 4, 2, 1]
    );
}

#[test]
fn deterministic_encoder_uses_all_fill_types() {
    let mut source = vec![7; 40];
    source.extend([1, 2].into_iter().cycle().take(41));
    source.extend(50_u8..100);
    source.extend_from_slice(b"uncompressed tail");
    let encoded = encode_lz2(&source);
    assert_eq!(decode_lz2(&encoded, 0x10000).unwrap(), source);
    assert!(encoded.len() < source.len());
    assert!(encoded.iter().any(|byte| byte & 0xe0 == 0xe0));
}

#[test]
fn maximum_command_length_round_trips() {
    let source = vec![0xaa; 2049];
    assert_eq!(decode_lz2(&encode_lz2(&source), 4096).unwrap(), source);
}

#[test]
fn deterministic_encoder_uses_all_dictionary_variants() {
    for (expected_command, source) in [
        (4, b"abcdWXYZabcdWXYZ".to_vec()),
        (
            5,
            [0x01_u8, 0x23, 0x45, 0x67, 0x80, 0xc4, 0xa2, 0xe6].to_vec(),
        ),
        (6, b"abcdWXYZZYXWdcba".to_vec()),
    ] {
        let encoded = encode_lz2(&source);
        assert_eq!(decode_lz2(&encoded, 0x1000).unwrap(), source);
        assert!(
            commands(&encoded).contains(&expected_command),
            "{encoded:02x?}"
        );
    }
}

#[test]
fn overlapping_dictionary_match_compresses_repetition() {
    let source = b"abcabcabcabcabcabcabcabc";
    let encoded = encode_lz2(source);
    assert_eq!(decode_lz2(&encoded, 0x1000).unwrap(), source);
    assert!(commands(&encoded).contains(&4));
    assert!(encoded.len() < source.len());
}

#[test]
fn prefix_decoder_reports_exact_stream_extent() {
    let stream = [0x01, 0x12, 0x34, 0xff, 0xaa, 0xbb];
    let decoded = decode_lz2_prefix(&stream, 2).unwrap();
    assert_eq!(decoded.bytes, [0x12, 0x34]);
    assert_eq!(decoded.consumed, 4);
    assert_eq!(
        decode_lz2(&stream, 2),
        Err(CodecError::TrailingCompressedData(2))
    );
}

#[test]
fn short_and_extended_header_boundaries_decode_exactly() {
    for length in [1_usize, 32, 33, 256, 1024] {
        let source = vec![0x5a; length];
        let encoded = encode_lz2(&source);
        let decoded = decode_lz2_prefix(&encoded, length).unwrap();
        assert_eq!(decoded.bytes, source);
        assert_eq!(decoded.consumed, encoded.len());
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
            for encoded in [encode_lz2(&source), encode_lz2_literals(&source)] {
                assert_eq!(decode_lz2(&encoded, source.len()).unwrap(), source);
                if !source.is_empty() {
                    assert_eq!(
                        decode_lz2(&encoded, source.len() - 1),
                        Err(CodecError::OutputLimitExceeded {
                            limit: source.len() - 1
                        })
                    );
                }
            }
        }
    }
}

#[test]
fn every_two_byte_input_is_a_total_decode_operation() {
    for first in 0_u8..=u8::MAX {
        for second in 0_u8..=u8::MAX {
            let _ = decode_lz2_prefix(&[first, second], 2048);
        }
    }
}

#[test]
fn reserved_extended_command_headers_are_rejected() {
    for header in 0xfc..=0xfe {
        assert_eq!(
            decode_lz2_prefix(&[header, 0x00, 0xff], 1),
            Err(CodecError::UnsupportedLz2Command(7))
        );
    }
}

fn commands(encoded: &[u8]) -> Vec<u8> {
    let mut cursor = 0;
    let mut commands = Vec::new();
    loop {
        let header = encoded[cursor];
        cursor += 1;
        if header == 0xff {
            break;
        }
        let (command, len) = decode_header(header, encoded, &mut cursor).unwrap();
        commands.push(command);
        cursor += match command {
            0 => len,
            1 | 3 => 1,
            2 | 4..=6 => 2,
            _ => unreachable!(),
        };
    }
    commands
}
