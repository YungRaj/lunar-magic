use super::*;
use crate::LevelPointerTable;
use lm_graphics::Bgr555;
use lm_level::{LevelObjectData, NativeSpriteStream};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum};

fn table(offset: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries: 1,
        stride: 3,
    }
}

fn layout() -> NativeLevelAssetsLayout {
    NativeLevelAssetsLayout {
        level: LevelRomLayout {
            mapper: Mapper::LoRom,
            layer1: table(0x20),
            sprites: table(0x30).into(),
            expanded_sprites: false,
        },
        palette: PaletteRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x40),
            colors_per_palette: 2,
        },
        exanimation: ExAnimationRomLayout {
            mapper: Mapper::LoRom,
            pointers: table(0x50),
            maximum_records: 8,
            maximum_encoded_len: 0x100,
        },
        expanded_settings: Some(ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x60,
            entries: 1,
            stride: ExpandedLevelSettingsRecord::ENCODED_LEN,
        }),
    }
}

fn policy() -> AllocationPolicy {
    AllocationPolicy {
        search: 0x100..0x8000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x20..0x53),
            ProtectedRange(0x60..0x80),
            ProtectedRange(0x7fdc..0x8000),
        ],
    }
}

fn options() -> NativeLevelAssetsSaveOptions {
    let allocation = policy();
    NativeLevelAssetsSaveOptions {
        level: LevelSaveOptions {
            layer1_allocation: allocation.clone(),
            sprite_allocation: allocation.clone(),
            previous_layer1: None,
            previous_sprites: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        palette: PaletteSaveOptions {
            allocation: allocation.clone(),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
        exanimation: ExAnimationSaveOptions {
            allocation,
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        },
    }
}

fn level() -> LoadedLevelSlot {
    LoadedLevelSlot {
        number: 0,
        layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]).unwrap(),
        sprites: NativeSpriteStream::parse(
            &[0x10, 0, 1, 2, 0xff],
            false,
            &SpriteLengthTable::standard(),
        )
        .unwrap(),
    }
}

fn animation() -> CompactExAnimation {
    CompactExAnimation {
        setting: 0,
        header_value: 0,
        trigger_mask: 0,
        trigger_values: [0; 16],
        records: Vec::new(),
    }
}

#[test]
fn native_assets_commit_as_one_history_operation_and_reopen() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let original = project.rom.logical_bytes().to_vec();
    let level = level();
    let palette = Palette {
        colors: vec![Bgr555(1), Bgr555(2)],
    };
    let animation = animation();
    let expanded = ExpandedLevelSettingsRecord::decode(&[0x5a; 0x20]).unwrap();
    let saved = project
        .save_native_level_assets(
            NativeLevelAssets {
                level: &level,
                palette: &palette,
                exanimation: &animation,
                expanded_settings: Some(&expanded),
            },
            layout(),
            &SpriteLengthTable::standard(),
            &[false; 256],
            0x7fdc,
            &options(),
        )
        .unwrap();
    assert_eq!(project.history.undo_len(), 1);
    assert_eq!(
        project
            .load_level_slot(0, layout().level, &SpriteLengthTable::standard())
            .unwrap(),
        level
    );
    assert_eq!(project.load_palette(0, layout().palette).unwrap(), palette);
    assert_eq!(
        project
            .load_expanded_level_settings(0, layout().expanded_settings.unwrap())
            .unwrap(),
        expanded
    );
    assert!(saved.expanded_settings_saved);
    assert_eq!(
        project
            .load_exanimation(0, layout().exanimation, &[false; 256])
            .unwrap(),
        animation
    );
    assert_ne!(
        saved.layer1.block.header_offset,
        saved.sprites.block.header_offset
    );
    assert_ne!(
        saved.palette.block.header_offset,
        saved.exanimation.block.header_offset
    );
    let expected = compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap();
    assert_eq!(
        SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
        expected
    );
    let committed = project.rom.logical_bytes().to_vec();
    project.history.undo(&mut project.rom).unwrap();
    assert_eq!(project.rom.logical_bytes(), original);
    project.history.redo(&mut project.rom).unwrap();
    assert_eq!(project.rom.logical_bytes(), committed);
}

#[test]
fn late_domain_validation_failure_preserves_rom_and_history() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let before = project.rom.logical_bytes().to_vec();
    let level = level();
    let bad_palette = Palette {
        colors: vec![Bgr555(1)],
    };
    assert!(
        project
            .save_native_level_assets(
                NativeLevelAssets {
                    level: &level,
                    palette: &bad_palette,
                    exanimation: &animation(),
                    expanded_settings: None,
                },
                NativeLevelAssetsLayout {
                    expanded_settings: None,
                    ..layout()
                },
                &SpriteLengthTable::standard(),
                &[false; 256],
                0x7fdc,
                &options(),
            )
            .is_err()
    );
    assert_eq!(project.rom.logical_bytes(), before);
    assert_eq!(project.history.undo_len(), 0);
}

#[test]
fn checksum_failure_after_staging_preserves_rom_and_history() {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let before = project.rom.logical_bytes().to_vec();
    let level = level();
    let palette = Palette {
        colors: vec![Bgr555(1), Bgr555(2)],
    };
    assert!(
        project
            .save_native_level_assets(
                NativeLevelAssets {
                    level: &level,
                    palette: &palette,
                    exanimation: &animation(),
                    expanded_settings: None,
                },
                NativeLevelAssetsLayout {
                    expanded_settings: None,
                    ..layout()
                },
                &SpriteLengthTable::standard(),
                &[false; 256],
                usize::MAX,
                &options(),
            )
            .is_err()
    );
    assert_eq!(project.rom.logical_bytes(), before);
    assert_eq!(project.history.undo_len(), 0);
}

#[test]
fn expanded_settings_pairing_and_allocation_protection_are_mandatory() {
    let level = level();
    let palette = Palette {
        colors: vec![Bgr555(1), Bgr555(2)],
    };
    let animation = animation();
    let expanded = ExpandedLevelSettingsRecord::decode(&[7; 0x20]).unwrap();
    let assets = NativeLevelAssets {
        level: &level,
        palette: &palette,
        exanimation: &animation,
        expanded_settings: Some(&expanded),
    };

    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
    let before = project.save_snapshot();
    assert!(matches!(
        project.save_native_level_assets(
            assets,
            NativeLevelAssetsLayout {
                expanded_settings: None,
                ..layout()
            },
            &SpriteLengthTable::standard(),
            &[false; 256],
            0x7fdc,
            &options(),
        ),
        Err(NativeLevelAssetsSaveError::ExpandedSettingsPairMismatch)
    ));
    assert_eq!(project.save_snapshot(), before);
    assert_eq!(project.history.undo_len(), 0);

    let mut unsafe_options = options();
    for policy in [
        &mut unsafe_options.level.layer1_allocation,
        &mut unsafe_options.level.sprite_allocation,
        &mut unsafe_options.palette.allocation,
        &mut unsafe_options.exanimation.allocation,
    ] {
        policy.protected.retain(|range| range.0 != (0x60..0x80));
    }
    assert!(matches!(
        project.save_native_level_assets(
            assets,
            layout(),
            &SpriteLengthTable::standard(),
            &[false; 256],
            0x7fdc,
            &unsafe_options,
        ),
        Err(NativeLevelAssetsSaveError::Payload(
            PayloadSaveError::ExtraWriteUnprotected { .. }
        ))
    ));
    assert_eq!(project.save_snapshot(), before);
    assert_eq!(project.history.undo_len(), 0);
}
