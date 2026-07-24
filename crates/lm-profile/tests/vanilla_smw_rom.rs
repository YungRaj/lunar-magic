use lm_level::SpriteLengthTable;
use lm_profile::{
    SMW_US_V1_LEVEL_LAYER1_POINTER_TABLE_OFFSET, SMW_US_V1_VANILLA_GRAPHICS_FILES,
    SMW_US_V1_VANILLA_LEVEL_SLOTS, smw_us_v1_vanilla_graphics_layout,
    smw_us_v1_vanilla_level_layout,
};
use lm_project::{GraphicsSaveOptions, LevelSaveOptions, Project};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::{fs, path::PathBuf};

#[test]
fn every_ordinary_graphics_file_in_the_local_reference_rom_decodes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("Super Mario World (USA).sfc");
    let Ok(bytes) = fs::read(path) else {
        return;
    };
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
fn every_native_level_slot_in_the_local_reference_rom_decodes() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("Super Mario World (USA).sfc");
    let Ok(bytes) = fs::read(path) else {
        return;
    };
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
fn pristine_layer1_edit_expands_repoints_and_reopens_without_touching_sprites() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("Super Mario World (USA).sfc");
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let mut project = Project::new(RomImage::from_bytes(bytes).unwrap());
    let layout = smw_us_v1_vanilla_level_layout();
    let mut level = project
        .load_level_slot(0, layout, &SpriteLengthTable::standard())
        .unwrap();
    let original_sprites = level.sprites.clone();
    let original_sprite_pointer = layout.sprites.read_snes_pointer(&project.rom, 0).unwrap();
    let replacement = (level.layer1.header.background_palette() + 1) & 7;
    level
        .layer1
        .header
        .set_background_palette(replacement)
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
    assert_eq!(reopened.sprites, original_sprites);
    assert_eq!(
        layout.sprites.read_snes_pointer(&project.rom, 0).unwrap(),
        original_sprite_pointer
    );
}

#[test]
fn pristine_graphics_edit_expands_repoints_and_reopens() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("Super Mario World (USA).sfc");
    let Ok(bytes) = fs::read(path) else {
        return;
    };
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
