use super::*;

#[test]
fn preserves_source_and_expands_default_object_modes() {
    let source = b"\xef\xbb\xbf10\t22\t0\tLine\\nTwo\r\n";
    let value = OscSidecar::decode(source).unwrap();
    assert_eq!(value.encode_lossless(), source);
    assert_eq!(value.entries()[0].selectors.len(), 5);
    assert_eq!(value.entries()[0].selectors[0].index, 0x10);
    assert_eq!(value.entries()[0].selectors[4].index, 0x110);
    assert_eq!(
        value.entries()[0].directive,
        OscDirective::Description("Line\nTwo".into())
    );
}

#[test]
fn decodes_selected_mode_special_families_and_display() {
    let value =
        OscSidecar::decode(b"12\t34\t33\t-8,16,1234;8,16,FFFF\n0\t20\t2\t0,0,10\n").unwrap();
    let selector = value.entries()[0].selectors[0];
    assert_eq!(selector.variant, 3);
    assert_eq!(selector.index, 0xd2);
    assert_eq!(
        value.entries()[0].directive,
        OscDirective::Display(vec![
            OscDisplayTile {
                x: -8,
                y: 16,
                tile: 0x1234,
            },
            OscDisplayTile {
                x: 8,
                y: 16,
                tile: 0x7fff,
            },
        ])
    );
    assert_eq!(value.entries()[1].selectors[0].index, 0x160);
}

#[test]
fn distinguishes_values_and_compact_attributes() {
    let value =
        OscSidecar::decode(b"1\t2\t8\t1,2,3,4,5,6,7,8;A,B,C,D,E,F,10,11\n1\t2\tA\t1,2,FF\n")
            .unwrap();
    assert_eq!(
        value.entries()[0].directive,
        OscDirective::Values(vec![
            [1, 2, 3, 4, 5, 6, 7, 8],
            [10, 11, 12, 13, 14, 15, 16, 17],
        ])
    );
    assert_eq!(
        value.entries()[1].directive,
        OscDirective::Attributes(vec![1, 2, 0xff])
    );
}

#[test]
fn clamps_linear_length_and_rejects_invalid_selected_mode() {
    let value = OscSidecar::decode(b"1\t2\t10000\ttext\n1\t2\t71\tbad\n").unwrap();
    assert_eq!(value.entries().len(), 1);
    assert_eq!(value.entries()[0].selectors[0].record_length, Some(2));
    assert_eq!(
        OscSidecar::decode(&vec![0; MAX_OSC_SOURCE_LEN + 1]),
        Err(OscSidecarError::SourceTooLarge(MAX_OSC_SOURCE_LEN + 1))
    );
}
