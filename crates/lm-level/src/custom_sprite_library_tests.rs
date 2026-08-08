use super::*;

fn record(bytes: &[u8]) -> SpriteRecord {
    SpriteRecord {
        encoded: bytes.to_vec(),
    }
}

#[test]
fn multi_sprite_placements_and_text_framing_round_trip() {
    let data = [
        0x5a, 0x01, 0x20, 0x10, 0x00, 0x30, 0x11, 0x05, 0x40, 0x12, 0xff,
    ];
    let text = b"\xef\xbb\xbfTwo sprites\r\nOne \xe2\x98\x83\r\n";
    let library = CustomSpriteLibrary::decode(&data, text, &SpriteLengthTable::standard()).unwrap();
    assert_eq!(library.header(), 0x5a);
    assert_eq!(library.entries().len(), 2);
    assert_eq!(library.entries()[0].sprites.len(), 2);
    assert_eq!(library.entries()[1].sprites.len(), 1);
    assert_eq!(library.entries()[1].description, "One ☃");
    assert_eq!(library.encode().unwrap(), (data.to_vec(), text.to_vec()));
}

#[test]
fn revision_length_table_controls_record_boundaries() {
    let mut lengths = SpriteLengthTable::standard();
    lengths.set(2, 0x44, 5).unwrap();
    let data = [0, 0x09, 2, 0x44, 0xaa, 0xbb, 0x01, 3, 4, 0xff];
    let library = CustomSpriteLibrary::decode(&data, b"Long\nShort", &lengths).unwrap();
    assert_eq!(library.entries()[0].sprites[0].encoded.len(), 5);
    assert_eq!(library.entries()[1].sprites[0].encoded.len(), 3);
    assert_eq!(library.encode().unwrap().0, data);
}

#[test]
fn synchronized_edits_are_atomic_and_search_is_unicode_aware() {
    let data = [0, 1, 2, 3, 5, 4, 5, 0xff];
    let mut library = CustomSpriteLibrary::decode(
        &data,
        b"Gr\xc3\xbcner Koopa\nPipe",
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    assert_eq!(library.search("GRÜN"), [0]);
    library.move_entry(0, 1).unwrap();
    assert_eq!(library.entries()[0].description, "Pipe");
    let before = library.clone();
    assert_eq!(
        library.insert(
            9,
            CustomSpriteEntry::new(vec![record(&[1, 2, 3])], "x".into()).unwrap()
        ),
        Err(CustomSpriteLibraryError::InvalidIndex(9))
    );
    assert_eq!(library, before);
}

#[test]
fn malformed_binary_and_text_boundaries_are_rejected() {
    let lengths = SpriteLengthTable::standard();
    assert_eq!(
        CustomSpriteLibrary::decode(&[], b"", &lengths),
        Err(CustomSpriteLibraryError::MissingHeader)
    );
    assert_eq!(
        CustomSpriteLibrary::decode(&[0, 1, 2], b"x", &lengths),
        Err(CustomSpriteLibraryError::MalformedSprite { offset: 1 })
    );
    assert_eq!(
        CustomSpriteLibrary::decode(&[0, 1, 2, 3], b"x", &lengths),
        Err(CustomSpriteLibraryError::MissingTerminator)
    );
    assert_eq!(
        CustomSpriteLibrary::decode(&[0, 0xff, 0], b"", &lengths),
        Err(CustomSpriteLibraryError::TrailingData(1))
    );
    assert!(matches!(
        CustomSpriteLibrary::decode(&[0, 1, 2, 3, 0xff], b"a\r\nb\n", &lengths),
        Err(CustomSpriteLibraryError::MixedLineEndings)
    ));
    assert!(CustomSpriteLibrary::decode(&[0, 1, 2, 3, 0xff], b"a\n", &lengths).is_ok());
}

#[test]
fn boundary_and_count_mismatches_are_explicit() {
    let lengths = SpriteLengthTable::standard();
    assert!(matches!(
        CustomSpriteLibrary::decode(&[0, 1, 2, 3, 5, 4, 5, 0xff], b"only one", &lengths),
        Err(CustomSpriteLibraryError::EntryCountMismatch {
            placements: 2,
            descriptions: 1
        })
    ));
    assert_eq!(
        CustomSpriteEntry::new(vec![record(&[0, 2, 3])], "x".into()),
        Ok(CustomSpriteEntry {
            sprites: vec![record(&[0, 2, 3])],
            description: "x".into()
        })
    );
    assert_eq!(
        CustomSpriteEntry::new(vec![record(&[1, 2, 3]), record(&[3, 4, 5])], "x".into()),
        Err(CustomSpriteLibraryError::UnexpectedPlacementBoundary)
    );
}

#[test]
fn first_placement_boundary_bit_is_ignored_but_retained() {
    let data = [0, 0, 2, 3, 0xff];
    let library =
        CustomSpriteLibrary::decode(&data, b"first", &SpriteLengthTable::standard()).unwrap();
    assert_eq!(library.encode().unwrap().0, data);
}

#[test]
fn original_picker_hides_only_the_unterminated_final_description() {
    let data = [0, 1, 2, 3, 5, 4, 5, 0xff];
    let terminated =
        CustomSpriteLibrary::decode(&data, b"one\ntwo\n", &SpriteLengthTable::standard()).unwrap();
    assert_eq!(
        terminated.lunar_magic_picker_entries(),
        terminated.entries()
    );

    let unterminated =
        CustomSpriteLibrary::decode(&data, b"one\ntwo", &SpriteLengthTable::standard()).unwrap();
    assert_eq!(unterminated.entries().len(), 2);
    assert_eq!(
        unterminated.lunar_magic_picker_entries(),
        &unterminated.entries()[..1]
    );
    assert_eq!(unterminated.encode().unwrap().1, b"one\ntwo");
}

#[test]
fn checked_encoding_rejects_programmatic_revision_length_mismatch() {
    let mut library = CustomSpriteLibrary::decode(
        &[0, 1, 2, 3, 0xff],
        b"first",
        &SpriteLengthTable::standard(),
    )
    .unwrap();
    library.entries[0].sprites[0].encoded.push(4);
    assert_eq!(
        library.encode_checked(&SpriteLengthTable::standard()),
        Err(CustomSpriteLibraryError::SpriteLengthMismatch {
            entry: 0,
            sprite: 0,
            expected: 3,
            actual: 4
        })
    );
}
