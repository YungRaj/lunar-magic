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
