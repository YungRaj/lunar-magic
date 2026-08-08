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

#[test]
fn every_selector_and_directive_mode_is_lossless_and_resolved() {
    // The two special families ignore the generic selected-variant grammar; all remaining
    // object types share the generic default/all-variants and selected-variant paths.
    for object_type in 0_u8..=0x3f {
        let special = matches!(object_type, 0 | 0x2d);
        for parameter in [0_u8, 0xff] {
            for selected_variant in [None, Some(0_u8), Some(1), Some(2), Some(3), Some(4)] {
                if special && selected_variant.is_some() {
                    continue;
                }
                for width in [0_u8, 0x0f] {
                    for height in [0_u8, 0x0f] {
                        for length in [0_u8, 1, 2, 15, 31] {
                            for alternate_linear in [false, true] {
                                for (directive_bits, payload) in [
                                    (0_u32, "description\\ntext"),
                                    (2, "-8,16,ffff;8,-16,0123"),
                                    (8, "1,2,3,4,5,6,7,8;7fff,0,a,10,20,30,40,50"),
                                    (10, "1,2,ff"),
                                ] {
                                    let variant_bits = selected_variant
                                        .map_or(0, |variant| 1 | u32::from(variant) << 4);
                                    let flags = directive_bits
                                        | u32::from(alternate_linear) << 2
                                        | variant_bits
                                        | u32::from(width) << 8
                                        | u32::from(height) << 12
                                        | u32::from(length) << 16;
                                    let source = format!(
                                        "\u{feff}{object_type:02x}\t{parameter:02x}\t{flags:x}\t{payload}\r\n"
                                    );
                                    let sidecar = OscSidecar::decode(source.as_bytes()).unwrap();
                                    assert_eq!(sidecar.encode_lossless(), source.as_bytes());
                                    let [entry] = sidecar.entries() else {
                                        panic!("selector variant did not decode: {source:?}")
                                    };
                                    let expected_variants: Vec<u8> = if special {
                                        vec![0]
                                    } else if let Some(variant) = selected_variant {
                                        vec![variant]
                                    } else {
                                        (0..5).collect()
                                    };
                                    assert_eq!(entry.selectors.len(), expected_variants.len());
                                    let resolved = crate::OscResolvedTable::from_sidecar(&sidecar);
                                    for (selector, variant) in
                                        entry.selectors.iter().zip(expected_variants)
                                    {
                                        assert_eq!(selector.object_type, object_type);
                                        assert_eq!(selector.parameter, parameter);
                                        assert_eq!(selector.variant, variant);
                                        let expected_index = match object_type {
                                            0 => 0x140 + u16::from(parameter),
                                            0x2d => 0x240 + u16::from(parameter),
                                            _ => u16::from(variant) * 0x40 + u16::from(object_type),
                                        };
                                        assert_eq!(selector.index, expected_index);
                                        assert_eq!(selector.width, width);
                                        assert_eq!(selector.height, height);
                                        assert_eq!(
                                            selector.record_length,
                                            (length != 0).then_some(length.clamp(2, 15))
                                        );
                                        assert_eq!(selector.alternate_linear, alternate_linear);
                                        let object = resolved.get(*selector).unwrap();
                                        match (&entry.directive, directive_bits) {
                                            (OscDirective::Description(value), 0) => {
                                                assert_eq!(value, "description\ntext");
                                                assert_eq!(
                                                    object.description.as_deref(),
                                                    Some(value.as_str())
                                                );
                                            }
                                            (OscDirective::Display(tiles), 2) => {
                                                assert_eq!(tiles.len(), 2);
                                                assert_eq!(tiles[0].tile, 0x7fff);
                                                assert_eq!(tiles[1].tile, 0x0123);
                                                assert_eq!(object.display.as_ref(), Some(tiles));
                                            }
                                            (OscDirective::Values(values), 8) => {
                                                assert_eq!(values.len(), 2);
                                                assert_eq!(
                                                    values[1],
                                                    [0x7fff, 0, 10, 16, 32, 48, 64, 80]
                                                );
                                                assert_eq!(object.values.as_ref(), Some(values));
                                            }
                                            (OscDirective::Attributes(values), 10) => {
                                                assert_eq!(values, &[1, 2, 0xff]);
                                                assert_eq!(
                                                    object.attributes.as_ref(),
                                                    Some(values)
                                                );
                                            }
                                            value => panic!("wrong directive variant: {value:?}"),
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
