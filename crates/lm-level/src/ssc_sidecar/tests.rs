use super::*;

#[test]
fn preserves_source_and_decodes_sprite_directives() {
    let source = b"\xef\xbb\xbf12\t123002\t-8,16,3C10;8,16,0123\r\n12\t0\tLine\\nTwo\n";
    let sidecar = SscSidecar::decode(source).unwrap();
    assert_eq!(sidecar.encode_lossless(), source);
    assert_eq!(sidecar.entries().len(), 2);
    let selector = sidecar.entries()[0].selector.unwrap();
    assert_eq!(selector.sprite_number, 0x12);
    assert_eq!(selector.record_length, Some(15));
    assert_eq!(selector.height, 3);
    assert_eq!(
        sidecar.entries()[0].directive,
        SscDirective::Display(vec![
            SscDisplayTile {
                x: -8,
                y: 16,
                tile: 0x3c10,
            },
            SscDisplayTile {
                x: 8,
                y: 16,
                tile: 0x0123,
            },
        ])
    );
    assert_eq!(
        sidecar.entries()[1].directive,
        SscDirective::Description("Line\nTwo".into())
    );
}

#[test]
fn decodes_palette_and_global_remaps() {
    let sidecar =
        SscSidecar::decode(b"20\t8\t1,2,3,4;A,B,C,D\n10000\t2\t10-12,20\n20000\t0\t30-31,7\n")
            .unwrap();
    assert_eq!(
        sidecar.entries()[0].directive,
        SscDirective::Palette(vec![[1, 2, 3, 4], [10, 11, 12, 13]])
    );
    assert_eq!(
        sidecar.entries()[1].directive,
        SscDirective::TileRemap {
            mode: 2,
            ranges: vec![SscRemapRange {
                first: 0x10,
                last: 0x12,
                target: 0x420,
            }],
        }
    );
    assert_eq!(
        sidecar.entries()[2].directive,
        SscDirective::PaletteRemap(vec![SscRemapRange {
            first: 0x30,
            last: 0x31,
            target: 7,
        }])
    );
}

#[test]
fn expands_display_text_macro_into_native_glyph_tiles() {
    let sidecar = SscSidecar::decode(b"1\t2\t4,-2,*AB*\n").unwrap();
    let SscDirective::Display(display) = &sidecar.entries()[0].directive else {
        panic!("expected display")
    };
    assert_eq!(
        display,
        &[
            SscDisplayTile {
                x: 4,
                y: -2,
                tile: 0x3c7c,
            },
            SscDisplayTile {
                x: 4,
                y: -2,
                tile: 0x3c41,
            },
            SscDisplayTile {
                x: 12,
                y: -2,
                tile: 0x3c42,
            },
        ]
    );
}

#[test]
fn skips_malformed_lines_and_bounds_source() {
    let sidecar = SscSidecar::decode(b"bad\n100\t0\tignored\n1\t2\tbad\n").unwrap();
    assert_eq!(sidecar.entries().len(), 1);
    assert_eq!(
        sidecar.entries()[0].directive,
        SscDirective::Display(Vec::new())
    );
    assert_eq!(
        SscSidecar::decode(&vec![0; MAX_SSC_SOURCE_LEN + 1]),
        Err(SscSidecarError::SourceTooLarge(MAX_SSC_SOURCE_LEN + 1))
    );
}

#[test]
fn every_selector_display_palette_and_remap_mode_is_lossless_and_resolved() {
    for sprite_number in [0_u8, 0xff] {
        for alternate in [false, true] {
            for extra_bits in 0_u8..=3 {
                for width in [0_u8, 0x0f] {
                    for height in [0_u8, 0x0f] {
                        for record in [0_u8, 1, 2, 3, 15, 31] {
                            for global_slot in [false, true] {
                                for (directive_flag, payload) in [
                                    (0_u32, "description\\ntext"),
                                    (2, "-8,16,ffff;8,-16,0123"),
                                    (8, "1,2,3,4;7fff,0,a,10"),
                                ] {
                                    // Bit 2 denotes a global selector and is not a description
                                    // directive in Lunar Magic's overloaded flag grammar.
                                    if global_slot && directive_flag == 0 {
                                        continue;
                                    }
                                    let flags = u32::from(alternate)
                                        | directive_flag
                                        | u32::from(global_slot) << 2
                                        | u32::from(extra_bits) << 4
                                        | u32::from(width) << 8
                                        | u32::from(height) << 12
                                        | u32::from(record) << 16;
                                    let source =
                                        format!("{sprite_number:02x}\t{flags:x}\t{payload}\r\n");
                                    let sidecar = SscSidecar::decode(source.as_bytes()).unwrap();
                                    assert_eq!(sidecar.encode_lossless(), source.as_bytes());
                                    let [entry] = sidecar.entries() else {
                                        panic!("selector variant did not decode: {source:?}")
                                    };
                                    let selector = entry.selector.unwrap();
                                    assert_eq!(selector.sprite_number, sprite_number);
                                    assert_eq!(selector.extra_bits, extra_bits);
                                    assert_eq!(
                                        selector.index,
                                        u16::from(sprite_number) + u16::from(extra_bits) * 0x100
                                    );
                                    assert_eq!(selector.width, width);
                                    assert_eq!(selector.height, height);
                                    assert_eq!(
                                        selector.record_length,
                                        (record != 0).then_some(record.clamp(3, 15))
                                    );
                                    assert_eq!(selector.alternate, alternate);
                                    assert_eq!(selector.global_slot, global_slot);
                                    match (&entry.directive, directive_flag) {
                                        (SscDirective::Description(value), 0) => {
                                            assert_eq!(value, "description\ntext")
                                        }
                                        (SscDirective::Display(tiles), 2) => {
                                            assert_eq!(tiles.len(), 2);
                                            assert_eq!(tiles[0].tile, 0x7fff);
                                            assert_eq!(tiles[1].tile, 0x0123);
                                        }
                                        (SscDirective::Palette(rows), 8) => {
                                            assert_eq!(rows, &[[1, 2, 3, 4], [0x7fff, 0, 10, 16]])
                                        }
                                        value => panic!("wrong directive variant: {value:?}"),
                                    }
                                    let resolved = crate::SscResolvedTable::from_sidecar(&sidecar);
                                    assert_eq!(resolved.get(selector).unwrap().selector, selector);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    for (mode, bias) in [(0, 0x2000), (1, 0), (2, 0x0400), (3, 0x0900)] {
        let source = format!("10000\t{mode:x}\t10-12,20\n20000\t0\t30-31,7\n");
        let sidecar = SscSidecar::decode(source.as_bytes()).unwrap();
        assert_eq!(sidecar.encode_lossless(), source.as_bytes());
        assert_eq!(
            sidecar.entries()[0].directive,
            SscDirective::TileRemap {
                mode,
                ranges: vec![SscRemapRange {
                    first: 0x10,
                    last: 0x12,
                    target: 0x20 + bias,
                }],
            }
        );
        let resolved = crate::SscResolvedTable::from_sidecar(&sidecar);
        assert_eq!(resolved.tile_remap(0x10), Some(0x20 + bias));
        assert_eq!(resolved.tile_remap(0x12), Some(0x20 + bias));
        assert_eq!(resolved.palette_remap(0x30), Some(7));
        assert_eq!(resolved.palette_remap(0x31), Some(7));
    }
}
