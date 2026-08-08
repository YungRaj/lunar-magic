use lm_app::{ControllerSnapshot, EditorMode, ExAnimationController, ExAnimationControllerEdit};
use lm_graphics::{CompactExAnimation, ExAnimationRecord};
use lm_project::{
    ChainedSnesPointerLocator, ExAnimationRomLayout, ExAnimationSaveOptions, GatedLayout,
    InstallationMarker, InstalledAsset, InstalledExAnimationRomLayout, InstalledLayout,
    LevelPointerTable, Project,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity, pc_to_snes};

const MODES: [bool; 256] = [false; 256];
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

fn write_pointer(bytes: &mut [u8], mapper: Mapper, offset: usize, target: usize) {
    let pointer = pc_to_snes(mapper, target).unwrap().to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
}

fn layout(mapper: Mapper, pointer_table: usize) -> ExAnimationRomLayout {
    ExAnimationRomLayout {
        mapper,
        pointers: LevelPointerTable {
            offset: pointer_table,
            entries: 2,
            stride: 3,
        },
        maximum_records: 32,
        maximum_encoded_len: 0x2000,
    }
}

fn installation(
    mapper: Mapper,
    pointer_table: usize,
) -> InstalledLayout<InstalledExAnimationRomLayout> {
    InstalledLayout::Alternatives {
        primary: GatedLayout {
            marker: InstallationMarker {
                offset: 0x80,
                expected: 0x22,
            },
            layout: InstalledExAnimationRomLayout {
                payload: layout(mapper, pointer_table),
                pointer_presence_mask: 0x00ff_ffff,
                pointer_locator: Some(ChainedSnesPointerLocator {
                    mapper,
                    first_operand_offset: 0x81,
                    final_operand_displacement: -0x20,
                }),
            },
        },
        fallback: None,
    }
}

fn record(kind: u8, trigger: u8, value: u8) -> ExAnimationRecord {
    ExAnimationRecord::new(
        kind,
        0,
        trigger,
        0x1234 + u16::from(value),
        true,
        &[value, value.wrapping_add(1)],
        false,
    )
    .unwrap()
}

fn animation(seed: u8) -> CompactExAnimation {
    let mut trigger_values = [0; 16];
    trigger_values[2] = seed.wrapping_add(9);
    CompactExAnimation {
        setting: seed & 7,
        header_value: 0x9234_5600 | u32::from(seed),
        trigger_mask: 4,
        trigger_values,
        records: vec![record(1, 0, seed), record(2, 0, seed.wrapping_add(4))],
    }
}

fn options(
    search: std::ops::Range<usize>,
    pointer_table: usize,
    runtime: usize,
) -> ExAnimationSaveOptions {
    ExAnimationSaveOptions {
        allocation: AllocationPolicy {
            search,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![
                ProtectedRange(0x80..0x84),
                ProtectedRange(pointer_table..pointer_table + 6),
                ProtectedRange(runtime - 0x20..runtime - 0x1d),
                ProtectedRange(runtime + 0x5c..runtime + 0x68),
                ProtectedRange(0x7fdc..0x7fe0),
            ],
        },
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    }
}

fn variant_rom(
    case: IdentityCase,
    pointer_table: usize,
    runtime: usize,
    copier_header: bool,
) -> Vec<u8> {
    let mapper = mapper(case.map_mode);
    let logical_len = if case.map_mode == 0x32 {
        0x40_8000
    } else {
        0x8000
    };
    let mut logical = vec![0xff; logical_len];
    logical[0x80] = 0x22;
    write_pointer(&mut logical, mapper, 0x81, runtime);
    write_pointer(&mut logical, mapper, runtime - 0x20, pointer_table);
    logical[runtime + 0x5c] = 0;
    logical[runtime + 0x65..runtime + 0x67].fill(0);

    let header = 0x7fc0;
    logical[header..header + 21].copy_from_slice(case.title);
    logical[header + 0x15] = case.map_mode;
    logical[header + 0x19] = case.region;
    logical[header + 0x1b] = 0;
    let checksum = compute_snes_checksum(&logical, header + 0x1c).unwrap();
    logical[header + 0x1c..header + 0x20].copy_from_slice(&checksum.encoded());

    let mut project = Project::new(RomImage::from_bytes(logical).unwrap());
    project
        .save_exanimation_with_checksum(
            1,
            &animation(4),
            layout(mapper, pointer_table),
            &MODES,
            header + 0x1c,
            &options(0x1000..0x3000, pointer_table, runtime),
        )
        .unwrap();
    project
        .save_installed_global_exanimation_with_checksum(
            &animation(12),
            installation(mapper, pointer_table),
            &MODES,
            header + 0x1c,
            &options(0x3000..0x5000, pointer_table, runtime),
        )
        .unwrap();
    let logical = project.rom.logical_bytes();
    if copier_header {
        let mut physical = COPIER_PREFIX.to_vec();
        physical.extend_from_slice(logical);
        physical
    } else {
        logical.to_vec()
    }
}

fn snapshot(bytes: Vec<u8>, revision: u64) -> ControllerSnapshot {
    let image = RomImage::from_bytes(bytes.clone()).unwrap();
    ControllerSnapshot {
        revision,
        mode: EditorMode::ExAnimation(1),
        identity: detect_identity(&image).unwrap(),
        document_path: None,
        rom_bytes: bytes,
    }
}

fn edit_variant(
    physical: Vec<u8>,
    pointer_table: usize,
    runtime: usize,
) -> (Vec<u8>, CompactExAnimation, CompactExAnimation) {
    let original = physical.clone();
    let identity = detect_identity(&RomImage::from_bytes(physical.clone()).unwrap()).unwrap();
    let mapper = identity.mapper;
    let level_layout = layout(mapper, pointer_table);
    let installed = installation(mapper, pointer_table);
    let mut project = Project::new(RomImage::from_bytes(physical.clone()).unwrap());

    let mut level =
        ExAnimationController::decode(&snapshot(physical, 7), level_layout, &MODES).unwrap();
    level
        .apply_edits(&[
            ExAnimationControllerEdit::SetSetting(7),
            ExAnimationControllerEdit::SetHeaderValue(0xdead_beef),
            ExAnimationControllerEdit::SetTrigger {
                trigger: 15,
                value: Some(0xaa),
            },
            ExAnimationControllerEdit::ReplaceRecord {
                index: 1,
                record: record(3, 0, 0x28),
            },
        ])
        .unwrap();
    let expected_level = level.animation().clone();
    let prepared = level
        .prepare_commit(
            "Variant per-level ExAnimation",
            &options(0x1000..0x3000, pointer_table, runtime),
        )
        .unwrap();
    project
        .apply_mutation("Variant per-level ExAnimation", &prepared.mutation)
        .unwrap();
    assert_eq!(
        project.load_exanimation(1, level_layout, &MODES).unwrap(),
        expected_level
    );
    let after_level = project.rom.as_file_bytes().to_vec();

    let mut global =
        ExAnimationController::decode_global(&snapshot(after_level.clone(), 8), installed, &MODES)
            .unwrap();
    global
        .apply_edits(&[
            ExAnimationControllerEdit::SetSetting(6),
            ExAnimationControllerEdit::SetHeaderValue(0x1234_abcd),
            ExAnimationControllerEdit::SetTrigger {
                trigger: 5,
                value: Some(0x44),
            },
            ExAnimationControllerEdit::InsertRecord {
                index: 1,
                record: record(3, 0, 0x38),
            },
        ])
        .unwrap();
    let expected_global = global.animation().clone();
    let prepared = global
        .prepare_commit(
            "Variant global ExAnimation",
            &options(0x3000..0x5000, pointer_table, runtime),
        )
        .unwrap();
    project
        .apply_mutation("Variant global ExAnimation", &prepared.mutation)
        .unwrap();
    assert_eq!(
        project
            .load_installed_global_exanimation(installed, &MODES)
            .unwrap(),
        InstalledAsset::Present(expected_global.clone())
    );
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let edited = project.rom.as_file_bytes().to_vec();

    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), after_level);
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), after_level);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), edited);
    (edited, expected_level, expected_global)
}

#[test]
fn level_and_global_edits_match_across_every_supported_identity_and_runtime_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for (pointer_table, runtime) in [(0x200, 0x6000), (0x240, 0x6800)] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, pointer_table, runtime, false);
                let headered = variant_rom(case, pointer_table, runtime, true);
                let (edited_headerless, level, global) =
                    edit_variant(headerless, pointer_table, runtime);
                let (edited_headered, headered_level, headered_global) =
                    edit_variant(headered, pointer_table, runtime);
                assert_eq!(headered_level, level);
                assert_eq!(headered_global, global);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}
