use super::*;
use crate::PaletteControllerEdit;
use lm_graphics::{Bgr555, PaletteChange};
use lm_level::{LegacyHeaderEdit, SpriteRecord, SpriteToken};
use lm_project::{
    ExAnimationRomLayout, ExAnimationSaveOptions, ExpandedLevelSettingsLayout, LevelPointerTable,
    LevelRomLayout, LevelSaveOptions, NativeLevelAssetsSaveOptions, PaletteRomLayout,
    PaletteSaveOptions, RatsOwnershipManifest,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{SnesChecksum, compute_snes_checksum, detect_identity, pc_to_snes};

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
            stride: 32,
        }),
    }
}

fn pointer(bytes: &mut [u8], offset: usize, target: usize) {
    let value = pc_to_snes(Mapper::LoRom, target).unwrap().to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&value[..3]);
}

fn snapshot() -> ControllerSnapshot {
    let mut bytes = vec![0xff; 0x8000];
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"SUPER MARIOWORLD     ");
    bytes[0x7fd5] = 0x20;
    bytes[0x7fd9] = 1;
    bytes[0x7fdb] = 0;
    pointer(&mut bytes, 0x20, 0x100);
    pointer(&mut bytes, 0x30, 0x120);
    pointer(&mut bytes, 0x40, 0x140);
    pointer(&mut bytes, 0x50, 0x160);
    bytes[0x100..0x109].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]);
    bytes[0x120..0x125].copy_from_slice(&[0x10, 0, 1, 2, 0xff]);
    bytes[0x140..0x144].copy_from_slice(&[1, 0, 2, 0]);
    bytes[0x160..0x187].fill(0);
    bytes[0x60..0x80].fill(0x5a);
    let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    let image = RomImage::from_bytes(bytes.clone()).unwrap();
    ControllerSnapshot {
        revision: 7,
        mode: EditorMode::Level(0),
        identity: detect_identity(&image).unwrap(),
        document_path: None,
        rom_bytes: bytes,
    }
}

fn options() -> NativeLevelAssetsSaveOptions {
    let allocation = AllocationPolicy {
        search: 0x200..0x7000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x20..0x53),
            ProtectedRange(0x60..0x80),
            ProtectedRange(0x7fc0..0x8000),
        ],
    };
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

fn tagged_snapshot() -> (ControllerSnapshot, RatsOwnershipManifest) {
    let original = snapshot();
    let mut project = Project::new(RomImage::from_bytes(original.rom_bytes).unwrap());
    let assets = project
        .load_native_level_assets(0, layout(), &SpriteLengthTable::standard(), &[false; 256])
        .unwrap();
    let saved = project
        .save_native_level_assets(
            assets.as_save_assets(),
            layout(),
            &SpriteLengthTable::standard(),
            &[false; 256],
            0x7fdc,
            &options(),
        )
        .unwrap();
    let bytes = project.save_snapshot();
    let image = RomImage::from_bytes(bytes.clone()).unwrap();
    (
        ControllerSnapshot {
            revision: 7,
            mode: EditorMode::Level(0),
            identity: detect_identity(&image).unwrap(),
            document_path: None,
            rom_bytes: bytes,
        },
        RatsOwnershipManifest {
            owned: vec![
                saved.layer1.block,
                saved.sprites.block,
                saved.palette.block,
                saved.exanimation.block,
            ],
            retained: Vec::new(),
        },
    )
}

#[test]
fn mixed_edits_prepare_one_checksum_valid_semantically_reopenable_commit() {
    let snapshot = snapshot();
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot,
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    controller
        .apply_edits(&[
            NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                vec![PaletteChange {
                    index: 1,
                    color: Bgr555(0x1234),
                }],
            )]),
            NativeLevelAssetsControllerEdit::ExAnimation(vec![
                ExAnimationControllerEdit::SetSetting(3),
            ]),
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(4, 0xabcd)]),
        ])
        .unwrap();
    let expected = controller.assets().clone();
    let prepared = controller
        .prepare_commit("mixed native assets", &options())
        .unwrap();
    assert_eq!(prepared.expected_revision, 7);
    assert!(!prepared.mutation.is_empty());
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("commit", &prepared.mutation)
        .unwrap();
    let reopened = project
        .load_native_level_assets(0, layout(), &SpriteLengthTable::standard(), &[false; 256])
        .unwrap();
    assert_eq!(reopened, expected);
    assert_eq!(
        SnesChecksum::decode(project.rom.logical_bytes(), 0x7fdc).unwrap(),
        compute_snes_checksum(project.rom.logical_bytes(), 0x7fdc).unwrap()
    );
}

#[test]
fn owned_aggregate_reclaims_four_payloads_keeps_direct_write_atomic_and_undoes() {
    let (snapshot, manifest) = tagged_snapshot();
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot,
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    assert_eq!(
        controller.previous_blocks,
        std::array::from_fn(|index| Some(manifest.owned[index].clone()))
    );
    controller
        .apply_edits(&[
            NativeLevelAssetsControllerEdit::Level(vec![
                NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(3)),
                NativeLevelEdit::ReplaceSprite {
                    index: 0,
                    token: SpriteToken::Record(SpriteRecord {
                        encoded: vec![0, 1, 9],
                    }),
                },
            ]),
            NativeLevelAssetsControllerEdit::Palette(vec![PaletteControllerEdit::ApplyChanges(
                vec![PaletteChange {
                    index: 1,
                    color: Bgr555(0x1234),
                }],
            )]),
            NativeLevelAssetsControllerEdit::ExAnimation(vec![
                ExAnimationControllerEdit::SetSetting(3),
            ]),
            NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(4, 0xabcd)]),
        ])
        .unwrap();
    let expected = controller.assets().clone();
    let prepared = controller
        .prepare_commit_with_reclamation("owned native assets", &options(), &manifest)
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(snapshot.rom_bytes).unwrap());
    project
        .apply_mutation("commit", &prepared.mutation)
        .unwrap();
    for block in &manifest.owned {
        assert!(
            project.rom.logical_bytes()[block.full_range()]
                .iter()
                .all(|byte| *byte == 0xff)
        );
    }
    assert_eq!(
        project
            .load_native_level_assets(0, layout(), &SpriteLengthTable::standard(), &[false; 256])
            .unwrap(),
        expected
    );
    project.undo().unwrap();
    for block in &manifest.owned {
        assert_eq!(
            lm_rats::parse_at(project.rom.logical_bytes(), block.header_offset).unwrap(),
            block.clone()
        );
    }
    assert_eq!(project.rom.read(0x60, 32).unwrap(), &[0x5a; 32]);
}

#[test]
fn late_cross_domain_failure_rolls_back_the_complete_aggregate() {
    let mut controller = NativeLevelAssetsController::decode(
        &snapshot(),
        layout(),
        &SpriteLengthTable::standard(),
        &[false; 256],
        PaletteOwnership::editable(2),
    )
    .unwrap();
    let before = controller.assets().clone();
    assert!(
        controller
            .apply_edits(&[
                NativeLevelAssetsControllerEdit::ExAnimation(vec![
                    ExAnimationControllerEdit::SetSetting(9),
                ]),
                NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(16, 1)]),
            ])
            .is_err()
    );
    assert_eq!(controller.assets(), &before);
    assert!(!controller.is_modified());
}
