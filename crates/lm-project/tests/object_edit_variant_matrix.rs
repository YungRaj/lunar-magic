use lm_level::{
    LevelObjectData, NativeSpriteStream, ObjectCoordinateNibbles, ObjectRecord, SpriteLengthTable,
};
use lm_project::{LevelPointerTable, LevelRomLayout, LevelSaveOptions, LoadedLevelSlot, Project};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity};

const COPIER_PREFIX: [u8; 512] = {
    let mut prefix = [0_u8; 512];
    prefix[0] = 0x40;
    prefix[8] = 0xaa;
    prefix[9] = 0xbb;
    prefix[10] = 0x04;
    prefix
};

#[derive(Clone, Copy)]
struct IdentityCase {
    title: &'static [u8; 21],
    region: u8,
    map_mode: u8,
}

fn mapper(map_mode: u8) -> Mapper {
    match map_mode {
        0x20 | 0x30 => Mapper::LoRom,
        0x23 => Mapper::Sa1,
        0x32 => Mapper::ExLoRom,
        _ => unreachable!(),
    }
}

fn layout(mapper: Mapper) -> LevelRomLayout {
    LevelRomLayout {
        mapper,
        layer1: LevelPointerTable {
            offset: 0x100,
            entries: 1,
            stride: 3,
        },
        sprites: LevelPointerTable {
            offset: 0x110,
            entries: 1,
            stride: 3,
        }
        .into(),
        expanded_sprites: false,
    }
}

fn variant_rom(case: IdentityCase, copier_header: bool) -> Vec<u8> {
    let logical_len = if case.map_mode == 0x32 {
        0x40_8000
    } else {
        0x8_0000
    };
    let mut logical = vec![0xff; logical_len];
    let header = 0x7fc0;
    logical[header..header + 21].copy_from_slice(case.title);
    logical[header + 0x15] = case.map_mode;
    logical[header + 0x19] = case.region;
    logical[header + 0x1b] = 0;
    let checksum = compute_snes_checksum(&logical, header + 0x1c).unwrap();
    logical[header + 0x1c..header + 0x20].copy_from_slice(&checksum.encoded());
    if copier_header {
        let mut physical = COPIER_PREFIX.to_vec();
        physical.extend(logical);
        physical
    } else {
        logical
    }
}

fn allocation(search: std::ops::Range<usize>) -> AllocationPolicy {
    AllocationPolicy {
        search,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![ProtectedRange(0x100..0x113), ProtectedRange(0x7fc0..0x7fe0)],
    }
}

fn source_level() -> LoadedLevelSlot {
    LoadedLevelSlot {
        number: 0,
        layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 0x09, 0x08, 0x07, 0x11, 0x22, 0x05, 0xff])
            .unwrap(),
        sprites: NativeSpriteStream::parse(
            &[0x10, 0x00, 0x20, 0x01, 0xff],
            false,
            &SpriteLengthTable::standard(),
        )
        .unwrap(),
    }
}

fn edit_variant(
    physical: Vec<u8>,
    expected_map_mode: u8,
    storage: std::ops::Range<usize>,
) -> (Vec<u8>, LoadedLevelSlot) {
    let original_prefix = physical[..physical.len() % 0x8000].to_vec();
    let image = RomImage::from_bytes(physical).unwrap();
    let identity = detect_identity(&image).unwrap();
    assert_eq!(identity.map_mode, expected_map_mode);
    assert_eq!(identity.mapper, mapper(expected_map_mode));
    let level_layout = layout(identity.mapper);
    let mut project = Project::new(image);
    let initial_options = LevelSaveOptions {
        layer1_allocation: allocation(storage.clone()),
        sprite_allocation: allocation(storage.clone()),
        previous_layer1: None,
        previous_sprites: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    let initial = project
        .save_level_slot_with_checksum(
            level_layout,
            &source_level(),
            &SpriteLengthTable::standard(),
            0x7fdc,
            &initial_options,
        )
        .unwrap();
    let before_edit = project.save_snapshot();
    let sprite_pointer_before = project.rom.read(0x110, 3).unwrap().to_vec();
    let mut expected = project
        .load_level_slot(0, level_layout, &SpriteLengthTable::standard())
        .unwrap();
    let removed_source = expected.layer1.objects.records[1].clone();

    let standard = expected
        .layer1
        .objects
        .insert_ordinary_object_at(
            ObjectRecord::new(vec![0x15, 0x17, 0]).unwrap(),
            1,
            ObjectCoordinateNibbles {
                first: 3,
                second: 4,
            },
        )
        .unwrap();
    expected
        .layer1
        .objects
        .insert_ordinary_object_at(
            ObjectRecord::new(vec![0, 0, 4]).unwrap(),
            2,
            ObjectCoordinateNibbles {
                first: 5,
                second: 6,
            },
        )
        .unwrap();
    expected
        .layer1
        .objects
        .relocate_ordinary_object(
            standard,
            3,
            ObjectCoordinateNibbles {
                first: 7,
                second: 8,
            },
        )
        .unwrap();
    let extended = expected
        .layer1
        .objects
        .records
        .iter_mut()
        .find(|record| record.command_id() == 0 && record.parameter() == 4)
        .unwrap();
    extended.set_parameter(0x10).unwrap();
    let removed = expected
        .layer1
        .objects
        .records
        .iter()
        .position(|record| record == &removed_source)
        .unwrap();
    expected.layer1.objects.records.remove(removed);

    let options = LevelSaveOptions {
        layer1_allocation: allocation(storage),
        sprite_allocation: initial_options.sprite_allocation,
        previous_layer1: Some(initial.layer1.block),
        previous_sprites: Some(initial.sprites.block),
        reuse_identical: true,
        erase_fill: 0xff,
    };
    project
        .save_level_layer1_with_checksum(level_layout, &expected, 0x7fdc, &options)
        .unwrap();
    let reopened = project
        .load_level_slot(0, level_layout, &SpriteLengthTable::standard())
        .unwrap();
    assert_eq!(reopened, expected);
    assert_eq!(project.rom.read(0x110, 3).unwrap(), sprite_pointer_before);
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    assert_eq!(
        &project.save_snapshot()[..original_prefix.len()],
        original_prefix
    );
    let edited = project.save_snapshot();
    assert!(project.undo().unwrap());
    assert_eq!(project.save_snapshot(), before_edit);
    assert!(project.redo().unwrap());
    assert_eq!(project.save_snapshot(), edited);
    (edited, reopened)
}

#[test]
fn object_edits_reopen_and_undo_across_every_supported_identity_mapper_header_and_storage_variant()
{
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for storage in [0x1_0000..0x1_8000, 0x2_0000..0x2_8000] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, false);
                let headered = variant_rom(case, true);
                let (edited_headerless, level) =
                    edit_variant(headerless, map_mode, storage.clone());
                let (edited_headered, headered_level) = edit_variant(headered, map_mode, storage);
                assert_eq!(headered_level, level);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}
