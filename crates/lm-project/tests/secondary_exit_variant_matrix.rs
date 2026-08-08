use lm_level::{SecondaryExit, SecondaryExitTable};
use lm_project::{Project, SecondaryExitPatchLocator, SecondaryExitStorage};
use lm_rats::{AllocationPolicy, ProtectedRange};
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

#[derive(Clone, Copy)]
enum StorageCase {
    Compact,
    AllTagged,
}

fn mapper(map_mode: u8) -> Mapper {
    match map_mode {
        0x20 | 0x30 => Mapper::LoRom,
        0x23 => Mapper::Sa1,
        0x32 => Mapper::ExLoRom,
        _ => unreachable!(),
    }
}

fn locator(mapper: Mapper) -> SecondaryExitPatchLocator {
    SecondaryExitPatchLocator {
        mapper,
        first_reader: 0x100,
        second_reader: 0x140,
        fixed_planes: [0x2000, 0x2400, 0x2800, 0x2c00],
    }
}

fn write_rats(bytes: &mut [u8], payload: usize, data: &[u8]) {
    let offset = payload - 8;
    let length = u16::try_from(data.len() - 1).unwrap();
    bytes[offset..offset + 4].copy_from_slice(b"STAR");
    bytes[offset + 4..offset + 6].copy_from_slice(&length.to_le_bytes());
    bytes[offset + 6..offset + 8].copy_from_slice(&(!length).to_le_bytes());
    bytes[payload..payload + data.len()].copy_from_slice(data);
}

fn write_pointer(bytes: &mut [u8], mapper: Mapper, offset: usize, target: usize) {
    let pointer = pc_to_snes(mapper, target).unwrap().to_le_bytes();
    bytes[offset..offset + 3].copy_from_slice(&pointer[..3]);
}

fn value(seed: u8) -> SecondaryExit {
    SecondaryExit {
        destination_level: 0x100 + u16::from(seed),
        position_and_method: seed & 7,
        screen: seed & 0x1f,
        x: seed & 0x0f,
        y: seed.wrapping_add(1) & 0x0f,
        destination_flags: seed & !0x08,
        x_and_overworld_flags: seed & 0xf0,
        additional_flags: seed.wrapping_add(2),
    }
}

fn initial_table(storage: StorageCase) -> SecondaryExitTable {
    let mut table = SecondaryExitTable {
        entries: vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT],
    };
    let index = match storage {
        StorageCase::Compact => 0x1ff,
        StorageCase::AllTagged => 0x400,
    };
    table.entries[index] = value(3);
    table
}

fn variant_rom(case: IdentityCase, storage: StorageCase, copier_header: bool) -> Vec<u8> {
    let mapper = mapper(case.map_mode);
    let logical_len = if case.map_mode == 0x32 {
        0x40_8000
    } else {
        0x8_0000
    };
    let mut logical = vec![0xff; logical_len];
    let locator = locator(mapper);
    let table = initial_table(storage);
    let encoded = table.encode().unwrap();
    let used_len = match storage {
        StorageCase::Compact => 0x200,
        StorageCase::AllTagged => 0x401,
    };
    let mut targets = [0_usize; 6];
    for plane in 0..6 {
        let start = plane * SecondaryExitTable::ENTRY_COUNT;
        if matches!(storage, StorageCase::Compact) && plane < 4 {
            targets[plane] = locator.fixed_planes[plane];
            logical[targets[plane]..targets[plane] + 0x200]
                .copy_from_slice(&encoded[start..start + 0x200]);
        } else {
            let payload = 0x1_0008 + plane * 0x1000;
            targets[plane] = payload;
            write_rats(&mut logical, payload, &encoded[start..start + used_len]);
        }
    }
    let first = [
        0xbf, 0, 0, 0, 0x85, 0x0e, 0x6b, 0xbf, 0, 0, 0, 0x85, 0x00, 0x6b, 0xbf, 0, 0, 0, 0x85,
        0x01, 0x6b,
    ];
    let second = [
        0xbf, 0, 0, 0, 0x6b, 0xbf, 0, 0, 0, 0x6b, 0xbf, 0, 0, 0, 0x6b,
    ];
    logical[locator.first_reader..locator.first_reader + first.len()].copy_from_slice(&first);
    logical[locator.second_reader..locator.second_reader + second.len()].copy_from_slice(&second);
    for (offset, target) in [
        (locator.first_reader + 1, targets[0]),
        (locator.first_reader + 8, targets[1]),
        (locator.first_reader + 15, targets[2]),
        (locator.second_reader + 1, targets[3]),
        (locator.second_reader + 6, targets[4]),
        (locator.second_reader + 11, targets[5]),
    ] {
        write_pointer(&mut logical, mapper, offset, target);
    }

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

fn options() -> AllocationPolicy {
    AllocationPolicy {
        search: 0x2_0000..0x6_0000,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(0x100..0x115),
            ProtectedRange(0x140..0x14f),
            ProtectedRange(0x2000..0x2e00),
            ProtectedRange(0x7fdc..0x7fe0),
        ],
    }
}

fn edit_variant(physical: Vec<u8>, storage: StorageCase) -> Vec<u8> {
    let original = physical.clone();
    let image = RomImage::from_bytes(physical).unwrap();
    let identity = detect_identity(&image).unwrap();
    let locator = locator(identity.mapper);
    let mut project = Project::new(image);
    let loaded = project.load_secondary_exit_table_detected(locator).unwrap();
    assert!(matches!(
        (&storage, &loaded.storage),
        (
            StorageCase::Compact,
            SecondaryExitStorage::Installed {
                fixed_prefix_planes: 4,
                used_len: 0x200,
                ..
            }
        ) | (
            StorageCase::AllTagged,
            SecondaryExitStorage::Installed {
                fixed_prefix_planes: 0,
                used_len: 0x401,
                ..
            }
        )
    ));
    let mut edited_table = loaded.table;
    let index = match storage {
        StorageCase::Compact => 0x101,
        StorageCase::AllTagged => 0x402,
    };
    edited_table.entries[index] = value(0x25);
    project
        .save_installed_secondary_exit_table(&edited_table, locator, &options(), 0x7fdc, 0xff)
        .unwrap();
    assert_eq!(
        project
            .load_secondary_exit_table_detected(locator)
            .unwrap()
            .table,
        edited_table
    );
    let after_edit = project.rom.as_file_bytes().to_vec();

    let cleared = SecondaryExitTable {
        entries: vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT],
    };
    project
        .save_installed_secondary_exit_table(&cleared, locator, &options(), 0x7fdc, 0xff)
        .unwrap();
    assert_eq!(
        project
            .load_secondary_exit_table_detected(locator)
            .unwrap()
            .table,
        cleared
    );
    assert!(detect_identity(&project.rom).unwrap().checksum_matches());
    let after_clear = project.rom.as_file_bytes().to_vec();
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), after_edit);
    assert!(project.undo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), original);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), after_edit);
    assert!(project.redo().unwrap());
    assert_eq!(project.rom.as_file_bytes(), after_clear);
    after_clear
}

#[test]
fn edit_and_clear_match_every_supported_mapper_header_and_storage_variant() {
    const SMW: &[u8; 21] = b"SUPER MARIOWORLD     ";
    const ALL_STARS_WORLD: &[u8; 21] = b"ALL_STARS + WORLD    ";
    let identities = [(SMW, 0), (SMW, 1), (ALL_STARS_WORLD, 1)];
    for &(title, region) in &identities {
        for map_mode in [0x20, 0x30, 0x23, 0x32] {
            for storage in [StorageCase::Compact, StorageCase::AllTagged] {
                let case = IdentityCase {
                    title,
                    region,
                    map_mode,
                };
                let headerless = variant_rom(case, storage, false);
                let headered = variant_rom(case, storage, true);
                let edited_headerless = edit_variant(headerless, storage);
                let edited_headered = edit_variant(headered, storage);
                assert_eq!(&edited_headered[..512], &COPIER_PREFIX);
                assert_eq!(&edited_headered[512..], edited_headerless);
            }
        }
    }
}
