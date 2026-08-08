use super::*;

fn object(bytes: &[u8]) -> ObjectRecord {
    ObjectRecord::new(bytes.to_vec()).unwrap()
}

#[test]
fn native_pair_round_trips_with_text_framing() {
    let data = [0, 0, 0, 0, 0, 0x01, 0x00, 0x03, 0x82, 0x08, 0x04, 0xff];
    let text = b"\xef\xbb\xbfGround \xe2\x89\x88\r\nPipe\r\n";
    let library = CustomObjectLibrary::decode(&data, text).unwrap();
    assert_eq!(library.entries().len(), 2);
    assert_eq!(library.entries()[0].description, "Ground ≈");
    assert_eq!(library.encode().unwrap(), (data.to_vec(), text.to_vec()));
}

#[test]
fn native_group_boundaries_create_multi_object_entries_and_round_trip() {
    let data = [
        1, 2, 3, 4, 5, // retained native header
        0x01, 0x00, 0x03, // first object in entry zero
        0x02, 0x08, 0x04, // second object in entry zero
        0x83, 0x00, 0x04, // first object in entry one: boundary bit
        0x04, 0x00, 0x03, // second object in entry one
        0xff,
    ];
    let library = CustomObjectLibrary::decode(&data, b"Pair zero\nPair one\n").unwrap();
    assert_eq!(library.data_header(), &[1, 2, 3, 4, 5]);
    assert_eq!(library.entries().len(), 2);
    assert_eq!(library.entries()[0].objects().count(), 2);
    assert_eq!(library.entries()[1].objects().count(), 2);
    assert!(
        library
            .entries()
            .iter()
            .flat_map(CustomObjectEntry::objects)
            .all(|object| !object.advances_screen())
    );
    assert_eq!(library.encode().unwrap().0, data);
}

#[test]
fn either_native_orientation_screen_bit_starts_a_group_and_round_trips() {
    let data = [
        1, 2, 3, 4, 5, 0x01, 0x00, 0x03, 0x13, 0x08, 0x04, // vertical next-screen/group bit
        0x04, 0x00, 0x03, 0xff,
    ];
    let library = CustomObjectLibrary::decode(&data, b"First\nVertical pair\n").unwrap();
    assert_eq!(library.entries().len(), 2);
    assert_eq!(library.entries()[0].objects().count(), 1);
    assert_eq!(library.entries()[1].objects().count(), 2);
    assert_eq!(library.encode().unwrap().0, data);
}

#[test]
fn programmatic_group_rejects_an_ambiguous_vertical_boundary_member() {
    assert_eq!(
        CustomObjectEntry::new_group(
            vec![object(&[1, 0, 3]), object(&[0x12, 8, 4])],
            "would split in Lunar Magic".into(),
        ),
        Err(CustomObjectLibraryError::InvalidGroupBoundary)
    );
}

#[test]
fn every_native_text_framing_variant_round_trips_exactly() {
    for utf8_bom in [false, true] {
        for line_ending in [LineEnding::Lf, LineEnding::CrLf] {
            for trailing_line_ending in [false, true] {
                let mut library = CustomObjectLibrary::default();
                library
                    .push(CustomObjectEntry::new(object(&[1, 0, 3]), "Ground ≈".into()).unwrap())
                    .unwrap();
                library
                    .push(CustomObjectEntry::new(object(&[2, 8, 4]), "Pipe".into()).unwrap())
                    .unwrap();
                let format = DescriptionFormat {
                    utf8_bom,
                    line_ending,
                    trailing_line_ending,
                };
                library.set_description_format(format).unwrap();
                let encoded = library.encode().unwrap();
                let decoded = CustomObjectLibrary::decode(&encoded.0, &encoded.1).unwrap();
                assert_eq!(decoded.description_format(), format);
                assert_eq!(decoded.entries(), library.entries());
                assert_eq!(decoded.encode().unwrap(), encoded);
            }
        }
    }
}

#[test]
fn synchronized_edits_search_and_failure_atomicity() {
    let mut library = CustomObjectLibrary::default();
    library
        .push(CustomObjectEntry::new(object(&[1, 0, 3]), "Blue Pipe".into()).unwrap())
        .unwrap();
    library
        .push(CustomObjectEntry::new(object(&[2, 8, 4]), "Grüner Hügel".into()).unwrap())
        .unwrap();
    assert_eq!(library.search("GRÜNER"), vec![1]);
    library.move_entry(1, 0).unwrap();
    assert_eq!(library.entries()[0].description, "Grüner Hügel");
    let before = library.clone();
    assert_eq!(
        library.insert(
            9,
            CustomObjectEntry::new(object(&[3, 0, 4]), "Nope".into()).unwrap()
        ),
        Err(CustomObjectLibraryError::InvalidIndex(9))
    );
    assert_eq!(library, before);
    let removed = library.remove(1).unwrap();
    assert_eq!(removed.description, "Blue Pipe");
}

#[test]
fn malformed_pairs_are_rejected() {
    assert_eq!(
        CustomObjectLibrary::decode(&[0, 0, 0, 0, 0, 1, 0, 3], b"one"),
        Err(CustomObjectLibraryError::MissingTerminator)
    );
    assert_eq!(
        CustomObjectLibrary::decode(&[0, 0, 0, 0, 0, 1, 0, 3, 0xff, 0], b"one"),
        Err(CustomObjectLibraryError::TrailingObjectBytes(1))
    );
    assert!(matches!(
        CustomObjectLibrary::decode(&[0, 0, 0, 0, 0, 1, 0, 3, 0xff], b"one\r\ntwo\n"),
        Err(CustomObjectLibraryError::MixedLineEndings)
    ));
    assert!(matches!(
        CustomObjectLibrary::decode(&[0, 0, 0, 0, 0, 1, 0, 3, 0xff], b"one\ntwo"),
        Err(CustomObjectLibraryError::EntryCountMismatch { .. })
    ));
}

#[test]
fn object_count_disambiguates_empty_final_description() {
    let one = CustomObjectLibrary::decode(&[0, 0, 0, 0, 0, 1, 0, 3, 0xff], b"named\n").unwrap();
    assert!(one.description_format().trailing_line_ending);
    let two = CustomObjectLibrary::decode(&[0, 0, 0, 0, 0, 1, 0, 3, 0x82, 8, 4, 0xff], b"named\n")
        .unwrap();
    assert!(!two.description_format().trailing_line_ending);
    assert_eq!(two.entries()[1].description, "");
    assert_eq!(two.encode().unwrap().1, b"named\n");
}

#[test]
fn every_data_truncation_and_sidecar_limits_fail() {
    let data = [0, 0, 0, 0, 0, 0x00, 0x00, 0x00, 0x12, 0xff];
    for length in 0..data.len() {
        assert!(CustomObjectLibrary::decode(&data[..length], b"entry").is_err());
    }
    assert_eq!(
        CustomObjectLibrary::decode(&vec![0; MAX_CUSTOM_OBJECT_SIDECAR_LEN + 1], b""),
        Err(CustomObjectLibraryError::DataTooLarge)
    );
    assert_eq!(
        CustomObjectEntry::new(object(&[1, 0, 3]), "x\ninvalid".into()),
        Err(CustomObjectLibraryError::InvalidDescription)
    );
}
