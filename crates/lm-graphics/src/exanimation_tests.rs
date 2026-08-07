use super::*;

#[test]
fn animation_set_round_trips_without_interpreting_unknown_fields() {
    let bytes: Vec<_> = (0..ExAnimationRecord::ENCODED_LEN * 2)
        .map(|index| index.to_le_bytes()[0])
        .collect();
    let set = ExAnimationSet::decode(&bytes, 2, 1).unwrap();
    assert_eq!(set.encode().unwrap(), bytes);
    assert_eq!(set.visible_slots, 1);
    assert!(ExAnimationSet::decode(&bytes, 1, 1).is_err());
    assert!(ExAnimationSet::decode(&bytes, 2, 3).is_err());
}

#[test]
fn fixed_set_encoding_validates_counts_before_allocation() {
    assert_eq!(
        checked_set_len(2, 1).unwrap(),
        2 * ExAnimationRecord::ENCODED_LEN
    );
    assert_eq!(
        checked_set_len(1, 2),
        Err(ExAnimationError::TooManyVisibleSlots {
            visible_slots: 2,
            record_count: 1,
        })
    );
    let record_count = usize::MAX / ExAnimationRecord::ENCODED_LEN + 1;
    assert_eq!(
        checked_set_len(record_count, 0),
        Err(ExAnimationError::SetSizeOverflow { record_count })
    );
}

#[test]
fn compact_records_round_trip_sparse_slots_and_triggers() {
    let mut active = ExAnimationRecord::inactive();
    active.bytes[0] = 2;
    active.bytes[1] = 2;
    active.bytes[2] = 7;
    active.bytes[4..7].copy_from_slice(&[0x34, 0x12, 1]);
    active.bytes[8..14].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
    let mut trigger_values = [0; 16];
    trigger_values[1] = 9;
    trigger_values[15] = 7;
    let compact = CompactExAnimation {
        setting: 3,
        header_value: 0x1234_5678,
        trigger_mask: 0x8002,
        trigger_values,
        records: vec![active, ExAnimationRecord::inactive()],
    };
    let sizes = [false; 256];
    let encoded = compact.encode(&sizes).unwrap();
    let (decoded, consumed) = CompactExAnimation::decode(&encoded, 32, &sizes).unwrap();
    assert_eq!(decoded.records.len(), 1);
    assert_eq!(
        decoded,
        CompactExAnimation {
            records: compact.records[..1].to_vec(),
            ..compact
        }
    );
    assert_eq!(consumed, encoded.len());
}

#[test]
fn special_animation_types_have_no_frame_payload() {
    let mut record = ExAnimationRecord::inactive();
    record.bytes[0] = 0x18;
    record.bytes[1] = 0xff;
    let compact = CompactExAnimation {
        setting: 0,
        header_value: 0,
        trigger_mask: 0,
        trigger_values: [0; 16],
        records: vec![record],
    };
    let encoded = compact.encode(&[false; 256]).unwrap();
    assert_eq!(encoded.len(), 8 + 2 + 5);
}

#[test]
fn semantic_record_constructor_validates_destination_and_frames() {
    let record = ExAnimationRecord::new(2, 2, 7, 0x1234, true, &[1, 2, 3, 4, 5, 6], false).unwrap();
    assert_eq!(record.kind(), 2);
    assert_eq!(record.frame_count_minus_one(), 2);
    assert_eq!(record.size_mode(), 7);
    assert_eq!(record.trigger(), 7);
    assert_eq!(record.destination(), 0x1234);
    assert!(record.destination_flag());
    assert_eq!(record.frame_bytes(false), [1, 2, 3, 4, 5, 6]);
    assert!(matches!(
        ExAnimationRecord::new(2, 2, 7, 0x8000, false, &[0; 6], false),
        Err(ExAnimationError::DestinationOutOfRange(0x8000))
    ));
    assert!(matches!(
        ExAnimationRecord::new(2, 2, 7, 0, false, &[0; 5], false),
        Err(ExAnimationError::WrongFrameSize {
            expected: 6,
            actual: 5
        })
    ));
}

#[test]
fn compact_encoding_rejects_state_that_would_disappear_on_reopen() {
    let mut disabled_trigger = CompactExAnimation {
        setting: 0,
        header_value: 0,
        trigger_mask: 0,
        trigger_values: [0; 16],
        records: Vec::new(),
    };
    disabled_trigger.trigger_values[3] = 9;
    assert_eq!(
        disabled_trigger.encode(&[false; 256]),
        Err(ExAnimationError::DisabledTriggerValue {
            trigger: 3,
            value: 9,
        })
    );

    let mut bytes = [0; ExAnimationRecord::ENCODED_LEN];
    bytes[0] = 2;
    bytes[2] = 7;
    bytes[3] = 0xaa;
    let record = ExAnimationRecord::decode(&bytes).unwrap();
    let animation = CompactExAnimation {
        trigger_values: [0; 16],
        records: vec![record],
        ..disabled_trigger
    };
    assert_eq!(
        animation.encode(&[false; 256]),
        Err(ExAnimationError::UnrepresentedRecordByte {
            record: 0,
            offset: 3,
            value: 0xaa,
        })
    );
}

#[test]
fn every_record_shape_round_trips_or_reports_exact_capacity_overflow() {
    let counts = [0, 1, 127, 128, 254, 255];
    for kind in 0_u8..=u8::MAX {
        for frame_count_minus_one in counts {
            for double_size in [false, true] {
                let declared = declared_compact_frame_len(kind, frame_count_minus_one, double_size);
                let payload = vec![kind; declared.min(0x200)];
                let result = ExAnimationRecord::new(
                    kind,
                    frame_count_minus_one,
                    if kind == 0 { 0 } else { 19 },
                    if kind == 0 { 0 } else { 0x7fff },
                    kind != 0,
                    &payload,
                    double_size,
                );
                if declared > 0x200 {
                    assert_eq!(
                        result,
                        Err(ExAnimationError::FramePayloadTooLarge {
                            actual: declared,
                            maximum: 0x200,
                        }),
                        "kind={kind:#04x}, count={}, double={double_size}",
                        usize::from(frame_count_minus_one) + 1
                    );
                    continue;
                }
                if kind == 0 && frame_count_minus_one != 0 {
                    assert!(matches!(
                        result,
                        Err(ExAnimationError::UnrepresentedRecordByte { offset: 1, .. })
                    ));
                    continue;
                }

                let record = result.unwrap();
                let animation = CompactExAnimation {
                    setting: 7,
                    header_value: 0x89ab_cdef,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: vec![record],
                };
                let mut modes = [false; 256];
                modes[if kind == 0 { 0 } else { 19 }] = double_size;
                let encoded = animation.encode(&modes).unwrap();
                let (decoded, consumed) = CompactExAnimation::decode(&encoded, 1, &modes).unwrap();
                let expected = if kind == 0 {
                    CompactExAnimation {
                        records: Vec::new(),
                        ..animation
                    }
                } else {
                    animation
                };
                assert_eq!(decoded, expected);
                assert_eq!(consumed, encoded.len());
            }
        }
    }
}

#[test]
fn decoder_rejects_double_width_count_before_truncated_payload() {
    let bytes = [1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 3, 128, 0, 0];
    let mut modes = [false; 256];
    modes[3] = true;
    assert_eq!(
        CompactExAnimation::decode(&bytes, 1, &modes),
        Err(ExAnimationError::FramePayloadTooLarge {
            actual: 516,
            maximum: 0x200,
        })
    );
}
