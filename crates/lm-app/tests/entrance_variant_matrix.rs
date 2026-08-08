use lm_app::{ControllerSnapshot, EditorMode, VanillaEntranceController, VanillaEntranceEdit};
use lm_level::{SeparateMidwayEntrance, SeparateMidwayEntranceTable};
use lm_project::{
    Project, SeparateMidwayPatchLocator, VanillaEntranceRomLayout, VanillaMainEntrance,
};
use lm_rom::{Mapper, RomImage, compute_snes_checksum, detect_identity, pc_to_snes};

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

fn layout(mapper: Mapper, table_base: usize) -> VanillaEntranceRomLayout {
    VanillaEntranceRomLayout {
        mapper,
        position_offset: table_base,
        vertical_settings_offset: table_base + 0x20,
        screen_and_method_offset: table_base + 0x40,
        level_mode_and_screen_offset: table_base + 0x60,
        entries: 2,
    }
}

fn write_rats_header(bytes: &mut [u8], payload: usize, payload_len: usize) {
    let offset = payload - 8;
    let length = u16::try_from(payload_len - 1).unwrap();
    bytes[offset..offset + 4].copy_from_slice(b"STAR");
    bytes[offset + 4..offset + 6].copy_from_slice(&length.to_le_bytes());
    bytes[offset + 6..offset + 8].copy_from_slice(&(!length).to_le_bytes());
}

fn write_pointer(bytes: &mut [u8], mapper: Mapper, offset: usize, target: usize) {
    let pointer = pc_to_snes(mapper, target).unwrap();
    let pointer = if mapper == Mapper::LoRom {
        pointer & 0x7f_ffff
    } else {
        pointer
    };
    let pointer = pointer.to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
}

fn variant_rom(
    case: IdentityCase,
    table_base: usize,
    hook: usize,
    helper: usize,
    midway_table: usize,
    copier_header: bool,
) -> Vec<u8> {
    let mapper = mapper(case.map_mode);
    let logical_len = if case.map_mode == 0x32 {
        0x40_8000
    } else {
        0x2_0000
    };
    let mut logical = vec![0xff; logical_len];
    let entrance_layout = layout(mapper, table_base);
    logical[entrance_layout.position_offset + 1] = 0x12;
    logical[entrance_layout.vertical_settings_offset + 1] = 0x23;
    logical[entrance_layout.screen_and_method_offset + 1] = 0x34;
    logical[entrance_layout.level_mode_and_screen_offset + 1] = 0x45;

    write_rats_header(&mut logical, helper, 0xd0);
    write_rats_header(
        &mut logical,
        midway_table,
        SeparateMidwayEntranceTable::ENCODED_LEN,
    );
    logical[helper..helper + 0xd0].fill(0);
    logical[helper] = 0x4a;
    logical[helper + 9] = 0xbf;
    logical[helper + 0x26] = 0xbf;
    logical[helper + 0x47] = 0xbf;
    logical[helper + 0x56] = 0xbf;
    logical[helper + 0xcc..helper + 0xd0].copy_from_slice(b"LM\x10\x01");
    for (pointer_offset, addend) in [(0x0a, 0), (0x27, 0x200), (0x57, 0x400), (0x48, 0x600)] {
        write_pointer(
            &mut logical,
            mapper,
            helper + pointer_offset,
            midway_table + addend,
        );
    }
    logical[hook] = 0x22;
    write_pointer(&mut logical, mapper, hook + 1, helper);
    logical[midway_table + 1] = 0x56;
    logical[midway_table + 0x200 + 1] = 0x67;
    logical[midway_table + 0x400 + 1] = 0x78;
    logical[midway_table + 0x600 + 1] = 0x89;

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

fn edit_variant(
    physical: Vec<u8>,
    table_base: usize,
    hook: usize,
) -> (Vec<u8>, VanillaMainEntrance, SeparateMidwayEntrance) {
    let original = physical.clone();
    let image = RomImage::from_bytes(physical.clone()).unwrap();
    let identity = detect_identity(&image).unwrap();
    let layout = layout(identity.mapper, table_base);
    let locator = SeparateMidwayPatchLocator {
        mapper: identity.mapper,
        hook_offset: hook,
    };
    let snapshot = ControllerSnapshot {
        revision: 6,
        mode: EditorMode::Level(1),
        identity,
        document_path: None,
        rom_bytes: physical.clone(),
    };
    let mut controller = VanillaEntranceController::decode_with_midway(&snapshot, layout, locator)
        .unwrap_or_else(|error| {
            panic!(
                "failed to decode {:?} entrance variant at hook {hook:#x}: {error:?}",
                layout.mapper
            )
        });
    let expected_main = VanillaMainEntrance {
        position: 0x5a,
        vertical_settings: 0xa5,
        screen_and_method: 0xc3,
        level_mode_and_screen: 0x3c,
    };
    let expected_main = VanillaMainEntrance {
        position: expected_main.position & 0x0f | 0xb0,
        ..expected_main
    };
    let expected_midway = SeparateMidwayEntrance {
        flags: 0xde,
        position: 0xad,
        additional_flags: 0xbe,
        high_position: 0xef,
    };
    controller
        .apply_edits(&[
            VanillaEntranceEdit::SetMain(VanillaMainEntrance {
                position: 0x5a,
                vertical_settings: 0xa5,
                screen_and_method: 0xc3,
                level_mode_and_screen: 0x3c,
            }),
            VanillaEntranceEdit::SetLayer2ScrollTable(0x0b),
            VanillaEntranceEdit::SetMidway(expected_midway),
        ])
        .unwrap();
    assert_eq!(controller.entrance(), expected_main);
    assert_eq!(controller.midway_entrance(), Some(expected_midway));
    let prepared = controller
        .prepare_commit("Entrance supported-variant matrix")
        .unwrap();
    let mut project = Project::new(RomImage::from_bytes(physical).unwrap());
    project
        .apply_mutation("Entrance supported-variant matrix", &prepared.mutation)
        .unwrap();
    assert_eq!(
        project.load_vanilla_main_entrance(1, layout).unwrap(),
        expected_main
    );
    assert_eq!(
        project
            .load_separate_midway_table(locator)
            .unwrap()
            .table
            .entries[1],
        expected_midway
    );
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let edited = project.rom.as_file_bytes().to_vec();
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), edited);
    (edited, expected_main, expected_midway)
}

#[test]
fn main_and_separate_midway_edits_match_every_supported_rom_and_storage_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for (table_base, hook, helper, midway_table) in [
                (0x200, 0x100, 0x1_0008, 0x1_1008),
                (0x400, 0x180, 0x1_2008, 0x1_3008),
            ] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, table_base, hook, helper, midway_table, false);
                let headered = variant_rom(case, table_base, hook, helper, midway_table, true);
                let (edited_headerless, main, midway) = edit_variant(headerless, table_base, hook);
                let (edited_headered, headered_main, headered_midway) =
                    edit_variant(headered, table_base, hook);
                assert_eq!(headered_main, main);
                assert_eq!(headered_midway, midway);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}
