use super::*;
use crate::{
    ExAnimationRomLayout, ExpandedLevelSettingsLayout, LevelPointerTable, LevelRomLayout,
    NativeLevelAssetsLayout, PaletteRomLayout,
};
use lm_level::ExpandedLevelSettingsRecord;
use lm_rom::{Mapper, RomImage, pc_to_snes};

fn pointer(bytes: &mut [u8], offset: usize, target: usize) {
    let snes = pc_to_snes(Mapper::LoRom, target).unwrap().to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&snes[..3]);
}

fn table(offset: usize) -> LevelPointerTable {
    LevelPointerTable {
        offset,
        entries: 1,
        stride: 3,
    }
}

fn layout(expanded: bool) -> NativeLevelAssetsLayout {
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
        expanded_settings: expanded.then_some(ExpandedLevelSettingsLayout {
            mapper: Mapper::LoRom,
            table_offset: 0x60,
            entries: 1,
            stride: ExpandedLevelSettingsRecord::ENCODED_LEN,
        }),
    }
}

fn project() -> Project {
    let mut bytes = vec![0xff; 0x8000];
    pointer(&mut bytes, 0x20, 0x100);
    pointer(&mut bytes, 0x30, 0x120);
    pointer(&mut bytes, 0x40, 0x140);
    pointer(&mut bytes, 0x50, 0x160);
    bytes[0x100..0x109].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]);
    bytes[0x120..0x125].copy_from_slice(&[0x10, 0, 1, 2, 0xff]);
    bytes[0x140..0x144].copy_from_slice(&[1, 0, 2, 0]);
    bytes[0x160..0x187].fill(0);
    bytes[0x60..0x80].fill(0x5a);
    Project::new(RomImage::from_bytes(bytes).unwrap())
}

#[test]
fn aggregate_load_decodes_all_declared_domains() {
    let project = project();
    let loaded = project
        .load_native_level_assets(
            0,
            layout(true),
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    assert_eq!(loaded.level.number, 0);
    assert_eq!(loaded.palette.colors.len(), 2);
    assert!(loaded.exanimation.records.is_empty());
    assert_eq!(
        loaded.expanded_settings.unwrap(),
        ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap()
    );
}

#[test]
fn aggregate_load_omits_an_undeclared_expanded_table() {
    let loaded = project()
        .load_native_level_assets(
            0,
            layout(false),
            &SpriteLengthTable::standard(),
            &[false; 256],
        )
        .unwrap();
    assert_eq!(loaded.expanded_settings, None);
}

#[test]
fn late_domain_failure_does_not_mutate_project() {
    let project = project();
    let snapshot = project.save_snapshot();
    let mut invalid = layout(true);
    invalid.exanimation.pointers.offset = usize::MAX;
    assert!(
        project
            .load_native_level_assets(0, invalid, &SpriteLengthTable::standard(), &[false; 256],)
            .is_err()
    );
    assert_eq!(project.save_snapshot(), snapshot);
    assert_eq!(project.history.undo_len(), 0);
}
