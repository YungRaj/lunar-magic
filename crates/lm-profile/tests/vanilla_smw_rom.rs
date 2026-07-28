use lm_level::{MwlFile, NativeLayer2Data, ObjectCoordinateNibbles, SpriteLengthTable};
use lm_profile::{
    SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET, SMW_US_V1_VANILLA_GRAPHICS_FILES,
    SMW_US_V1_VANILLA_LEVEL_SLOTS, load_smw_us_v1_level_map16_base,
    smw_us_v1_object_tileset_graphics_files, smw_us_v1_sprite_tileset_graphics_files,
    smw_us_v1_vanilla_graphics_layout, smw_us_v1_vanilla_layer2_layout,
    smw_us_v1_vanilla_level_layout, smw_us_v1_vanilla_special_graphics_layout,
};
use lm_project::{GraphicsSaveOptions, LevelSaveOptions, Project};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum};
use std::{collections::BTreeMap, fs, path::PathBuf};

fn pristine_smw_us_rom_bytes() -> Vec<u8> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for path in [
        root.join("Super Mario World (USA).sfc"),
        root.join("SMW-working.sfc"),
        root.join("sysLMRestore/smwOrig.smc"),
    ] {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let Ok(image) = RomImage::from_bytes(bytes.clone()) else {
            continue;
        };
        if image.logical_len() == 0x8_0000
            && lm_rom::detect_identity(&image).is_ok_and(|identity| {
                identity.game == lm_rom::SupportedGame::SuperMarioWorld
                    && identity.region == lm_rom::Region::NorthAmerica
                    && identity.revision == 0
                    && identity.checksum_matches()
            })
        {
            return bytes;
        }
    }
    panic!("verified pristine SMW-US fixture not found");
}

#[test]
fn pristine_level_001_layer2_upper_plane_matches_lunar_magic_export() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = pristine_smw_us_rom_bytes();
    let project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let level = project
        .load_level_slot(
            1,
            smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        )
        .unwrap();
    let NativeLayer2Data::Tilemap(actual) = project
        .load_level_layer2(
            1,
            level.layer1.header.level_mode(),
            smw_us_v1_vanilla_layer2_layout(),
        )
        .unwrap()
    else {
        panic!("level 001 must use a shared background");
    };
    let expected_planes = MwlFile::decode(
        &fs::read(root.join("oracle-work/lm363/pristine-us/levels/Level 001.mwl")).unwrap(),
    )
    .unwrap()
    .layer2_section()
    .unwrap()
    .payload;
    for y in 0..16 {
        for x in 0..32 {
            let actual_index = lm_level::native_layer2_tilemap_index(x, y).unwrap() * 2;
            let expected_index = (y * 32 + x) * 2;
            assert_eq!(
                &actual[actual_index..actual_index + 2],
                &expected_planes[expected_index..expected_index + 2],
                "Layer 2 tile ({x}, {y}) differs from Lunar Magic's export"
            );
        }
    }
}

#[test]
fn every_ordinary_graphics_file_in_the_local_reference_rom_decodes() {
    let bytes = pristine_smw_us_rom_bytes();
    let project = Project::new(RomImage::from_bytes(bytes).unwrap());
    for file_number in 0..SMW_US_V1_VANILLA_GRAPHICS_FILES {
        let graphics = project
            .load_graphics_file(file_number, smw_us_v1_vanilla_graphics_layout())
            .unwrap_or_else(|error| panic!("failed to decode GFX{file_number:02X}: {error}"));
        assert!(
            !graphics.tiles.is_empty(),
            "GFX{file_number:02X} decoded no tiles"
        );
    }
}

#[test]
fn special_graphics_pointers_match_gfx33_and_gfx32_in_the_reference_rom() {
    let bytes = pristine_smw_us_rom_bytes();
    let project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let layout = smw_us_v1_vanilla_special_graphics_layout();
    assert_eq!(layout.read_pointer(&project, 0).unwrap().get(), 0x08_bfc0);
    assert_eq!(layout.read_pointer(&project, 1).unwrap().get(), 0x08_8000);
    for file_number in 0..2 {
        let graphics = project
            .load_graphics_file(file_number, layout)
            .unwrap_or_else(|error| {
                panic!("failed to decode special GFX{file_number:02X}: {error}")
            });
        assert!(!graphics.tiles.is_empty());
    }
}

#[test]
fn every_object_tileset_graphics_assignment_decodes_in_the_reference_rom() {
    let bytes = pristine_smw_us_rom_bytes();
    let project = Project::new(RomImage::from_bytes(bytes).unwrap());
    for tileset in 0..16 {
        let files = smw_us_v1_object_tileset_graphics_files(&project.rom, tileset).unwrap();
        for file in files {
            project
                .load_graphics_file(file, smw_us_v1_vanilla_graphics_layout())
                .unwrap_or_else(|error| {
                    panic!("tileset {tileset:X} GFX{file:02X} failed: {error}")
                });
        }
    }
}

#[test]
fn every_sprite_tileset_graphics_assignment_decodes_in_the_reference_rom() {
    let bytes = pristine_smw_us_rom_bytes();
    let project = Project::new(RomImage::from_bytes(bytes).unwrap());
    assert_eq!(
        smw_us_v1_sprite_tileset_graphics_files(&project.rom, 0).unwrap(),
        [0x00, 0x01, 0x13, 0x02]
    );
    assert_eq!(
        smw_us_v1_sprite_tileset_graphics_files(&project.rom, 8).unwrap(),
        [0x00, 0x01, 0x13, 0x20]
    );
    for tileset in 0..32 {
        let files = smw_us_v1_sprite_tileset_graphics_files(&project.rom, tileset).unwrap();
        for file in files {
            project
                .load_graphics_file(file, smw_us_v1_vanilla_graphics_layout())
                .unwrap_or_else(|error| {
                    panic!("sprite tileset {tileset:02X} GFX{file:02X} failed: {error}")
                });
        }
    }
}

#[test]
fn every_native_level_slot_in_the_local_reference_rom_decodes() {
    let bytes = pristine_smw_us_rom_bytes();
    let project = Project::new(RomImage::from_bytes(bytes).unwrap());
    for level in 0..SMW_US_V1_VANILLA_LEVEL_SLOTS {
        project
            .load_level_slot(
                level,
                smw_us_v1_vanilla_level_layout(),
                &SpriteLengthTable::standard(),
            )
            .unwrap_or_else(|error| panic!("failed to decode level {level:03X}: {error}"));
    }
}

#[test]
fn pristine_level_map16_sources_resolve_in_the_local_reference_rom() {
    let bytes = pristine_smw_us_rom_bytes();
    let rom = RomImage::from_bytes(bytes).unwrap();
    let loaded = load_smw_us_v1_level_map16_base(&rom, 0).unwrap();
    assert_eq!(loaded.tileset_source_offset, 0x68b70);
    assert_eq!(loaded.common_source_offset, 0x68000);
    assert_eq!(loaded.tileset_tiles, 178);
    assert_eq!(loaded.common_tiles, 334);
    assert_ne!(loaded.bytes, [0; 4096]);
}

#[test]
fn pristine_layer1_edit_expands_repoints_and_reopens_without_touching_sprites() {
    let bytes = pristine_smw_us_rom_bytes();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let layout = smw_us_v1_vanilla_level_layout();
    let mut level = project
        .load_level_slot(0, layout, &SpriteLengthTable::standard())
        .unwrap();
    let original_sprites = level.sprites.clone();
    let original_sprite_pointer = layout.sprites.read_snes_pointer(&project.rom, 0).unwrap();
    let replacement = (level.layer1.header.background_palette() + 1) & 7;
    let replacement_tileset = (level.layer1.header.object_tileset() + 1) & 0x0f;
    level
        .layer1
        .header
        .set_background_palette(replacement)
        .unwrap();
    level
        .layer1
        .header
        .set_object_tileset(replacement_tileset)
        .unwrap();
    let original_coordinates = level.layer1.objects.records[0].coordinate_nibbles();
    let replacement_coordinates = ObjectCoordinateNibbles {
        first: (original_coordinates.first + 1) & 0x0f,
        second: original_coordinates.second,
    };
    level.layer1.objects.records[0]
        .set_coordinate_nibbles(replacement_coordinates)
        .unwrap();

    project
        .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
        .unwrap();
    let allocation = AllocationPolicy {
        search: 0x80_000..0x10_0000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x7fc0..0x8000),
            ProtectedRange(
                SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET
                    ..SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET
                        + SMW_US_V1_VANILLA_LEVEL_SLOTS * 3,
            ),
        ],
    };
    project
        .save_level_layer1_with_checksum(
            layout,
            &level,
            0x7fdc,
            &LevelSaveOptions {
                layer1_allocation: allocation.clone(),
                sprite_allocation: allocation,
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();

    let reopened = project
        .load_level_slot(0, layout, &SpriteLengthTable::standard())
        .unwrap();
    assert_eq!(reopened.layer1.header.background_palette(), replacement);
    assert_eq!(reopened.layer1.header.object_tileset(), replacement_tileset);
    assert_eq!(
        reopened.layer1.objects.records[0].coordinate_nibbles(),
        replacement_coordinates
    );
    assert_eq!(reopened.sprites, original_sprites);
    assert_eq!(
        layout.sprites.read_snes_pointer(&project.rom, 0).unwrap(),
        original_sprite_pointer
    );
}

#[test]
fn pristine_unique_sprite_stream_edits_in_place_and_reopens() {
    let bytes = pristine_smw_us_rom_bytes();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let before = project.save_snapshot();
    let layout = smw_us_v1_vanilla_level_layout();
    let lengths = SpriteLengthTable::standard();
    let mut owners = BTreeMap::<u32, Vec<usize>>::new();
    for number in 0..SMW_US_V1_VANILLA_LEVEL_SLOTS {
        let pointer = layout
            .sprites
            .read_snes_pointer(&project.rom, number)
            .unwrap();
        owners.entry(pointer.get()).or_default().push(number);
    }
    let original = (0..SMW_US_V1_VANILLA_LEVEL_SLOTS)
        .filter(|number| {
            let pointer = layout
                .sprites
                .read_snes_pointer(&project.rom, *number)
                .unwrap();
            owners[&pointer.get()].len() == 1
        })
        .find_map(|number| {
            let level = project.load_level_slot(number, layout, &lengths).ok()?;
            (!level.sprites.tokens.is_empty()).then_some(level)
        })
        .expect("reference ROM should contain a uniquely owned non-empty sprite stream");
    let pointer_before = layout
        .sprites
        .read_snes_pointer(&project.rom, original.number)
        .unwrap();
    let mut replacement = original.clone();
    replacement.sprites.header ^= 1;
    replacement.sprites.tokens.pop();

    project
        .save_level_sprites_in_place_with_checksum(
            layout,
            &original,
            &replacement,
            &lengths,
            0x7fdc,
        )
        .unwrap();
    assert_eq!(
        project
            .load_level_slot(original.number, layout, &lengths)
            .unwrap()
            .sprites,
        replacement.sprites
    );
    assert_eq!(
        layout
            .sprites
            .read_snes_pointer(&project.rom, original.number)
            .unwrap(),
        pointer_before
    );
    assert_eq!(
        SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
        compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
    );
    assert!(project.undo().unwrap());
    assert_eq!(project.save_snapshot(), before);
}

#[test]
fn pristine_graphics_edit_expands_repoints_and_reopens() {
    let bytes = pristine_smw_us_rom_bytes();
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let layout = smw_us_v1_vanilla_graphics_layout();
    let mut graphics = project.load_graphics_file(0, layout).unwrap();
    let old = graphics.tiles[0].pixel(0, 0).unwrap();
    let replacement = (old + 1) & 0x0f;
    graphics.tiles[0].set_pixel(0, 0, replacement).unwrap();
    project
        .expand_rom(Mapper::LoRom, 0x10_0000, 0xff, 0x7fdc)
        .unwrap();
    let planes = layout.split_pointer_planes.unwrap();
    let plane = |offset| ProtectedRange(offset..offset + planes.entries);
    project
        .save_graphics_file_with_checksum(
            0,
            &graphics,
            layout,
            0x7fdc,
            &GraphicsSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x80_000..0x10_0000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![
                        plane(planes.low_offset),
                        plane(planes.high_offset),
                        plane(planes.bank_offset),
                    ],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    assert_eq!(
        project.load_graphics_file(0, layout).unwrap().tiles[0].pixel(0, 0),
        Some(replacement)
    );
    assert!(
        layout
            .read_pointer(&project, 0)
            .unwrap()
            .to_pc(Mapper::LoRom)
            .unwrap()
            >= 0x80_000
    );
}
