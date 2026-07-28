use crate::RevisionProfile;
#[cfg(test)]
use crate::RevisionProfileError;
use lm_level::SpriteLengthTable;
use lm_project::{
    CompleteOverworldRomLayout, CompleteOverworldShape, EndpointRomLayout, EventRevealRomLayout,
    ExAnimationRomLayout, ExpandedLevelSettingsLayout, GraphicsRomLayout, LevelPointerTable,
    LevelRomLayout, Map16RomLayout, MessageRomLayout, OverworldLayersRomLayout, PaletteRomLayout,
    SpriteRomLayout,
};
use lm_rom::{Mapper, Region, SupportedGame};
#[cfg(test)]
use lm_rom::{RomIdentity, SnesChecksum};
#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
pub(crate) fn pristine_smw_us_rom_bytes() -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
        root.join("Super Mario World (USA).sfc"),
        root.join("SMW-working.sfc"),
        root.join("sysLMRestore/smwOrig.smc"),
    ] {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let Ok(image) = lm_rom::RomImage::from_bytes(bytes.clone()) else {
            continue;
        };
        if image.logical_len() != 0x8_0000 {
            continue;
        }
        let Ok(identity) = lm_rom::detect_identity(&image) else {
            continue;
        };
        if identity.game == SupportedGame::SuperMarioWorld
            && identity.region == Region::NorthAmerica
            && identity.revision == 0
            && identity.checksum_matches()
        {
            return bytes;
        }
    }
    panic!("verified pristine SMW-US fixture not found");
}

#[cfg(test)]
fn identity() -> RomIdentity {
    RomIdentity {
        game: SupportedGame::SuperMarioWorld,
        mapper: Mapper::ExLoRom,
        region: Region::NorthAmerica,
        revision: 0,
        map_mode: 0x20,
        cartridge_type: 0x02,
        internal_header_offset: 0x7fc0,
        stored_checksum: SnesChecksum {
            complement: 0xffff,
            checksum: 0,
        },
        computed_checksum: SnesChecksum {
            complement: 0xffff,
            checksum: 0,
        },
    }
}

fn pointer(offset: usize, entries: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries,
        stride: 3,
    }
}

fn expanded_settings(mapper: Mapper) -> ExpandedLevelSettingsLayout {
    ExpandedLevelSettingsLayout {
        mapper,
        table_offset: 0x2_0000,
        entries: 0x200,
        stride: 0x20,
    }
}

fn palette_installation(mapper: Mapper) -> lm_project::InstalledLayout<PaletteRomLayout> {
    lm_project::InstalledLayout::Unconditional(PaletteRomLayout {
        mapper,
        pointers: pointer(0x1400, 0x200),
        colors_per_palette: 256,
    })
}

fn exanimation_installation(
    mapper: Mapper,
) -> lm_project::InstalledLayout<lm_project::InstalledExAnimationRomLayout> {
    lm_project::InstalledLayout::Unconditional(lm_project::InstalledExAnimationRomLayout {
        payload: ExAnimationRomLayout {
            mapper,
            pointers: pointer(0x1a00, 0x200),
            maximum_records: 32,
            maximum_encoded_len: 0x8000,
        },
        pointer_presence_mask: 0x00ff_0000,
        pointer_locator: None,
    })
}

fn animation_modes() -> [bool; 256] {
    let mut modes = [false; 256];
    modes[7] = true;
    modes[255] = true;
    modes
}

#[must_use]
pub fn profile() -> RevisionProfile {
    let mapper = Mapper::ExLoRom;
    let mut sprite_lengths = SpriteLengthTable::standard();
    sprite_lengths.set(2, 0x7b, 5).unwrap();
    let modes = animation_modes();
    let shape = CompleteOverworldShape {
        width: 32,
        height: 32,
        event_reveals: 16,
        endpoints: 8,
        messages: 8,
        sprites: 8,
        sprite_record_len: 9,
        palette_colors: 256,
    };
    RevisionProfile {
        name: "Audited test revision".into(),
        game: SupportedGame::SuperMarioWorld,
        region: Region::NorthAmerica,
        revision: 0,
        mapper,
        level: LevelRomLayout {
            mapper,
            layer1: pointer(0x100, 0x200),
            sprites: pointer(0x700, 0x200).into(),
            expanded_sprites: true,
        },
        layer2: None,
        map16: Map16RomLayout {
            mapper,
            graphics: pointer(0xd00, 0x80),
            acts_like: pointer(0xf00, 0x80),
        },
        graphics: GraphicsRomLayout {
            mapper,
            pointers: pointer(0x1100, 0x100),
            split_pointer_planes: None,
            compression: lm_project::GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        },
        palette: PaletteRomLayout {
            mapper,
            pointers: pointer(0x1400, 0x200),
            colors_per_palette: 256,
        },
        palette_installation: palette_installation(mapper),
        exanimation: ExAnimationRomLayout {
            mapper,
            pointers: pointer(0x1a00, 0x200),
            maximum_records: 32,
            maximum_encoded_len: 0x8000,
        },
        exanimation_installation: exanimation_installation(mapper),
        expanded_settings: Some(expanded_settings(mapper)),
        overworld: CompleteOverworldRomLayout {
            layers: OverworldLayersRomLayout {
                mapper,
                layer1: pointer(0x2000, 0x200),
                layer2: pointer(0x2600, 0x200),
                width: shape.width,
                height: shape.height,
            },
            event_reveals: EventRevealRomLayout {
                mapper,
                sources: pointer(0x2c00, 0x200),
                destinations: pointer(0x3200, 0x200),
                entries_per_slot: shape.event_reveals,
            },
            endpoints: EndpointRomLayout {
                mapper,
                pointers: pointer(0x3800, 0x200),
                endpoints_per_slot: shape.endpoints,
            },
            messages: MessageRomLayout {
                mapper,
                pointers: pointer(0x3e00, 0x200),
                messages_per_slot: shape.messages,
            },
            sprites: SpriteRomLayout {
                mapper,
                pointers: pointer(0x4400, 0x200),
                sprites_per_slot: shape.sprites,
                record_len: shape.sprite_record_len,
            },
            palette: PaletteRomLayout {
                mapper,
                pointers: pointer(0x4a00, 0x200),
                colors_per_palette: shape.palette_colors,
            },
            animation: ExAnimationRomLayout {
                mapper,
                pointers: pointer(0x5000, 0x200),
                maximum_records: 64,
                maximum_encoded_len: 0x8000,
            },
        },
        overworld_shape: shape,
        sprite_lengths,
        exanimation_double_size_modes: modes,
    }
}

#[test]
fn canonical_profile_round_trips_every_controller_input() {
    let expected = profile();
    expected.validate().unwrap();
    let encoded = expected.encode();
    let actual = RevisionProfile::parse(&encoded).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.encode(), encoded);
}

#[test]
fn split_sprite_pointer_encodings_round_trip_canonically() {
    let mut shared = profile();
    let low_words = LevelPointerTable {
        stride: 2,
        ..shared.level.sprites.low_or_contiguous_table()
    };
    shared.level.sprites = lm_project::SpritePointerTable::SplitSharedBank {
        low_words,
        bank_offset: 0x2_8000,
    };
    let encoded = shared.encode();
    assert!(encoded.contains("level.sprites.encoding=split-shared-bank\n"));
    assert_eq!(RevisionProfile::parse(&encoded).unwrap(), shared);

    let mut parallel = profile();
    parallel.level.sprites = lm_project::SpritePointerTable::SplitBankTable {
        low_words,
        banks: LevelPointerTable {
            offset: 0x2_8000,
            entries: low_words.entries,
            stride: 1,
        },
    };
    let encoded = parallel.encode();
    assert!(encoded.contains("level.sprites.encoding=split-bank-table\n"));
    assert_eq!(RevisionProfile::parse(&encoded).unwrap(), parallel);
}

#[test]
fn optional_layer2_layout_round_trips_canonically() {
    let mut expected = profile();
    expected.layer2 = Some(lm_project::LevelLayer2RomLayout {
        mapper: expected.mapper,
        pointers: pointer(0x2_9000, 0x200),
        background_bank_substitution: None,
        descriptor_table: None,
        maximum_compressed_len: 0x8000,
        tilemap_encoding: lm_project::LevelLayer2TilemapEncoding::Legacy { high_byte: 1 },
    });
    let encoded = expected.encode();
    assert!(encoded.contains("level.layer2.tilemap_encoding=legacy\n"));
    assert!(encoded.contains("level.layer2.high_byte=0x01\n"));
    assert_eq!(RevisionProfile::parse(&encoded).unwrap(), expected);

    expected.layer2.as_mut().unwrap().tilemap_encoding =
        lm_project::LevelLayer2TilemapEncoding::SplitPlanes;
    let encoded = expected.encode();
    assert!(!encoded.contains("level.layer2.high_byte="));
    assert_eq!(RevisionProfile::parse(&encoded).unwrap(), expected);
}

#[test]
fn marker_gated_optional_asset_layouts_round_trip_canonically() {
    let mut expected = profile();
    expected.palette_installation = lm_project::InstalledLayout::Alternatives {
        primary: lm_project::GatedLayout {
            marker: lm_project::InstallationMarker {
                offset: 0x2_8800,
                expected: 0xc2,
            },
            layout: expected.palette,
        },
        fallback: None,
    };
    expected.exanimation_installation = lm_project::InstalledLayout::Alternatives {
        primary: lm_project::GatedLayout {
            marker: lm_project::InstallationMarker {
                offset: 0x2_8810,
                expected: 0x22,
            },
            layout: lm_project::InstalledExAnimationRomLayout {
                payload: expected.exanimation,
                pointer_presence_mask: 0x00ff_ff00,
                pointer_locator: Some(lm_project::ChainedSnesPointerLocator {
                    mapper: expected.mapper,
                    first_operand_offset: 0x2_8811,
                    final_operand_displacement: -0x86,
                }),
            },
        },
        fallback: Some(lm_project::GatedLayout {
            marker: lm_project::InstallationMarker {
                offset: 0x2_8820,
                expected: 0x22,
            },
            layout: lm_project::InstalledExAnimationRomLayout {
                payload: expected.exanimation,
                pointer_presence_mask: 0x00ff_0000,
                pointer_locator: None,
            },
        }),
    };
    let encoded = expected.encode();
    assert!(encoded.contains("palette.installation=marker\n"));
    assert!(encoded.contains("exanimation.installation=alternatives\n"));
    assert!(encoded.contains("exanimation.primary_locator_operand_offset=0x28811\n"));
    assert!(encoded.contains("exanimation.primary_locator_displacement=-0x86\n"));
    assert_eq!(RevisionProfile::parse(&encoded).unwrap(), expected);

    expected.palette_installation = lm_project::InstalledLayout::Absent;
    expected.exanimation_installation = lm_project::InstalledLayout::Absent;
    assert_eq!(
        RevisionProfile::parse(&expected.encode()).unwrap(),
        expected
    );
}

#[test]
fn installation_metadata_rejects_incomplete_or_inapplicable_fields() {
    let valid = profile().encode();
    assert!(matches!(
        RevisionProfile::parse(&valid.replace(
            "palette.installation=unconditional",
            "palette.installation=absent\npalette.marker_offset=0x20"
        )),
        Err(RevisionProfileError::IncompleteInstallationLayout(
            "palette"
        ))
    ));
    assert!(matches!(
        RevisionProfile::parse(
            &valid.replace(
                "exanimation.installation=unconditional",
                "exanimation.installation=alternatives"
            )
        ),
        Err(RevisionProfileError::MissingKey(key))
            if key == "exanimation.primary_marker_offset"
    ));
    assert!(matches!(
        RevisionProfile::parse(&valid.replace(
            "exanimation.primary_pointer_mask=0xff0000",
            "exanimation.primary_pointer_mask=0"
        )),
        Err(RevisionProfileError::InvalidPointerPresenceMask)
    ));
}

#[test]
fn lz3_graphics_policy_round_trips_and_unknown_policies_fail() {
    let mut expected = profile();
    expected.graphics.compression = lm_project::GraphicsCompression::Lz3;
    let encoded = expected.encode();
    assert!(encoded.contains("graphics.compression=lz3\n"));
    assert_eq!(RevisionProfile::parse(&encoded).unwrap(), expected);
    assert!(matches!(
        RevisionProfile::parse(&encoded.replace("graphics.compression=lz3", "graphics.compression=x")),
        Err(RevisionProfileError::InvalidGraphicsCompression(value)) if value == "x"
    ));
}

#[test]
fn parser_rejects_unknown_duplicate_missing_and_bad_values() {
    let valid = profile().encode();
    let unknown = format!("{valid}surprise=1\n");
    assert!(matches!(
        RevisionProfile::parse(&unknown),
        Err(RevisionProfileError::UnknownKey { .. })
    ));
    let duplicate = format!("{valid}name=again\n");
    assert!(matches!(
        RevisionProfile::parse(&duplicate),
        Err(RevisionProfileError::DuplicateKey(_))
    ));
    let missing = valid
        .lines()
        .filter(|line| !line.starts_with("palette.colors="))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(
        RevisionProfile::parse(&missing),
        Err(RevisionProfileError::MissingKey(_))
    ));
    let bad = valid.replace("level.expanded_sprites=1", "level.expanded_sprites=perhaps");
    assert!(matches!(
        RevisionProfile::parse(&bad),
        Err(RevisionProfileError::InvalidBoolean { .. })
    ));
}

#[test]
fn parser_rejects_corrupt_recovered_tables() {
    let valid = profile().encode();
    let short = valid.replace(&"03".repeat(SpriteLengthTable::ENCODED_LEN), "0303");
    // The customized table means the exact all-3 sequence is absent; truncate its actual field.
    let short = short
        .lines()
        .map(|line| {
            line.strip_prefix("sprite_lengths=").map_or_else(
                || line.to_owned(),
                |value| format!("sprite_lengths={}", &value[..value.len() - 2]),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(
        RevisionProfile::parse(&short),
        Err(RevisionProfileError::InvalidTableLength {
            key: "sprite_lengths",
            ..
        })
    ));
    let malformed = valid
        .lines()
        .map(|line| {
            line.strip_prefix("exanimation_double_size_modes=")
                .map_or_else(
                    || line.to_owned(),
                    |value| format!("exanimation_double_size_modes=z{}", &value[1..]),
                )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(
        RevisionProfile::parse(&malformed),
        Err(RevisionProfileError::InvalidHex { .. })
    ));
}

#[test]
fn validation_rejects_unsafe_pointer_shapes_and_mapper_disagreement() {
    let mut invalid = profile();
    invalid.level.layer1.stride = 2;
    assert!(matches!(
        invalid.validate(),
        Err(RevisionProfileError::InvalidPointerStride { .. })
    ));
    let mut invalid = profile();
    invalid.palette.mapper = Mapper::LoRom;
    assert!(matches!(
        invalid.validate(),
        Err(RevisionProfileError::MapperMismatch {
            domain: "palette",
            ..
        })
    ));
    let mut invalid = profile();
    invalid.level.layer1.offset = 0x7f_0000;
    assert!(matches!(
        invalid.validate(),
        Err(RevisionProfileError::UnmappedPointerTable("level.layer1"))
    ));
}

#[test]
fn validation_rejects_shape_disagreement_and_invalid_sprite_lengths() {
    let mut invalid = profile();
    invalid.overworld.layers.width += 1;
    assert_eq!(
        invalid.validate(),
        Err(RevisionProfileError::OverworldShapeMismatch)
    );
    let mut bytes = *profile().sprite_lengths.encoded();
    bytes[55] = 2;
    let mut invalid = profile();
    invalid.sprite_lengths = SpriteLengthTable::decode(&bytes).unwrap();
    assert_eq!(
        invalid.validate(),
        Err(RevisionProfileError::InvalidSpriteLength)
    );
}

#[test]
fn profile_is_bound_to_stable_detected_rom_identity() {
    let profile = profile();
    let expected = identity();
    assert!(profile.matches_identity(&expected));
    profile.ensure_identity(&expected).unwrap();

    for mismatch in [
        RomIdentity {
            region: Region::Japan,
            ..expected.clone()
        },
        RomIdentity {
            game: SupportedGame::AllStarsAndWorld,
            ..expected.clone()
        },
        RomIdentity {
            revision: 1,
            ..expected.clone()
        },
        RomIdentity {
            mapper: Mapper::LoRom,
            ..expected
        },
    ] {
        assert!(!profile.matches_identity(&mismatch));
        assert!(matches!(
            profile.ensure_identity(&mismatch),
            Err(RevisionProfileError::IdentityMismatch { .. })
        ));
    }
}

#[test]
fn parser_rejects_invalid_identity_fields() {
    let valid = profile().encode();
    assert!(matches!(
        RevisionProfile::parse(&valid.replace("game=super-mario-world", "game=unknown")),
        Err(RevisionProfileError::InvalidGame(_))
    ));
    assert!(matches!(
        RevisionProfile::parse(&valid.replace("region=north-america", "region=mars")),
        Err(RevisionProfileError::InvalidRegion(_))
    ));
    assert!(matches!(
        RevisionProfile::parse(&valid.replace("revision=0", "revision=256")),
        Err(RevisionProfileError::InvalidNumber { .. })
    ));
}

#[test]
fn parser_enforces_resource_bounds_before_field_decoding() {
    let too_large = "x".repeat(RevisionProfile::MAX_TEXT_LEN + 1);
    assert!(matches!(
        RevisionProfile::parse(&too_large),
        Err(RevisionProfileError::TextTooLong { .. })
    ));

    let long_line = format!(
        "{}\n#{}",
        RevisionProfile::MAGIC,
        "x".repeat(RevisionProfile::MAX_LINE_LEN + 1)
    );
    assert!(matches!(
        RevisionProfile::parse(&long_line),
        Err(RevisionProfileError::LineTooLong { line: 2, .. })
    ));

    let many_lines = format!(
        "{}\n{}",
        RevisionProfile::MAGIC,
        "#\n".repeat(RevisionProfile::MAX_LINES)
    );
    assert!(matches!(
        RevisionProfile::parse(&many_lines),
        Err(RevisionProfileError::TooManyLines { .. })
    ));
}

#[test]
fn canonical_name_limit_is_exact_and_programmatic_validation_matches_parse() {
    let mut boundary = profile();
    boundary.name = "n".repeat(RevisionProfile::MAX_NAME_LEN);
    boundary.validate().unwrap();
    assert_eq!(
        RevisionProfile::parse(&boundary.encode()).unwrap(),
        boundary
    );

    let mut oversized = profile();
    oversized.name = "n".repeat(RevisionProfile::MAX_NAME_LEN + 1);
    assert!(matches!(
        oversized.validate(),
        Err(RevisionProfileError::NameTooLong { .. })
    ));
    assert!(matches!(
        RevisionProfile::parse(&oversized.encode()),
        Err(RevisionProfileError::NameTooLong { .. })
    ));
}

#[test]
fn pointer_table_entry_count_is_resource_bounded() {
    let mut invalid = profile();
    invalid.level.layer1.entries = RevisionProfile::MAX_POINTER_TABLE_ENTRIES + 1;
    assert!(matches!(
        invalid.validate(),
        Err(RevisionProfileError::PointerTableEntryLimit {
            domain: "level.layer1",
            ..
        })
    ));
}

#[test]
fn overlapping_pointer_tables_are_rejected_before_rom_access() {
    let mut invalid = profile();
    invalid.map16.graphics.offset = invalid.level.layer1.offset + 1;
    assert!(matches!(
        invalid.validate(),
        Err(RevisionProfileError::OverlappingPointerTables {
            first: "level.layer1",
            second: "map16.graphics",
        })
    ));
}

#[test]
fn expanded_settings_layout_is_bounded_and_cannot_overlap_pointer_tables() {
    let mut invalid = profile();
    invalid.expanded_settings.as_mut().unwrap().stride = 31;
    assert!(matches!(
        invalid.validate(),
        Err(RevisionProfileError::InvalidExpandedSettingsLayout)
    ));

    let mut invalid = profile();
    invalid.expanded_settings.as_mut().unwrap().table_offset = invalid.level.layer1.offset;
    assert!(matches!(
        invalid.validate(),
        Err(RevisionProfileError::ExpandedSettingsTableOverlap {
            pointer_table: "level.layer1"
        })
    ));
}

#[test]
fn legacy_profiles_without_expanded_settings_remain_canonical() {
    let encoded = profile().encode();
    let legacy = encoded
        .lines()
        .filter(|line| !line.starts_with("expanded_settings."))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let parsed = RevisionProfile::parse(&legacy).unwrap();
    assert_eq!(parsed.expanded_settings, None);
    assert_eq!(parsed.encode(), legacy);

    let partial = legacy.replace(
        "level.expanded_sprites=1",
        "expanded_settings.offset=0x20000\nlevel.expanded_sprites=1",
    );
    assert!(matches!(
        RevisionProfile::parse(&partial),
        Err(RevisionProfileError::IncompleteExpandedSettingsLayout)
    ));
}

#[test]
fn installed_layer2_descriptor_layout_round_trips_and_requires_all_fields() {
    let mut expected = profile();
    expected.layer2 = Some(lm_project::LevelLayer2RomLayout {
        mapper: expected.mapper,
        pointers: pointer(0x2_9000, 0x200),
        background_bank_substitution: None,
        descriptor_table: Some(lm_project::LevelLayer2DescriptorTable {
            offset: 0x2_8800,
            entries: 0x200,
            stride: 1,
        }),
        maximum_compressed_len: 0x8000,
        tilemap_encoding: lm_project::LevelLayer2TilemapEncoding::Legacy { high_byte: 0 },
    });
    let encoded = expected.encode();
    assert!(encoded.contains("level.layer2.descriptor_offset=0x28800\n"));
    assert_eq!(RevisionProfile::parse(&encoded).unwrap(), expected);

    let partial = encoded
        .lines()
        .filter(|line| !line.starts_with("level.layer2.descriptor_stride="))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(matches!(
        RevisionProfile::parse(&partial),
        Err(RevisionProfileError::IncompleteLayer2Layout)
    ));
}
