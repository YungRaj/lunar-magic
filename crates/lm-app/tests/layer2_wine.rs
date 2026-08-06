use lm_level::{MwlFile, MwlLayer2Descriptor, MwlSectionKind, NativeLayer2Data};
use lm_project::{
    LevelLayer2RomLayout, LevelLayer2SaveOptions, LevelLayer2TilemapEncoding, LevelPointerTable,
    Project,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn wine_path(path: &Path) -> String {
    let rendered = path.display().to_string().replace('/', r"\");
    format!(r"Z:\{}", rendered.trim_start_matches('\\'))
}

#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixture"]
fn lunar_magic_reexports_rust_checksum_atomic_layer2_tilemap_edit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let installed = root.join("oracle-work/lm363/pristine-us/level-save-105/after.smc");
    let directory = std::env::temp_dir().join(format!(
        "lm-layer2-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("Rust Layer 2 edit.smc");
    let exported_mwl = directory.join("Level 105 reexported.mwl");

    let layout = LevelLayer2RomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: 0x2e600,
            entries: 0x200,
            stride: 3,
        },
        background_bank_substitution: None,
        legacy_pointer_redirect: None,
        descriptor_table: Some(lm_project::LevelLayer2DescriptorTable {
            offset: 0x77310,
            entries: 0x200,
            stride: 1,
        }),
        maximum_compressed_len: 0x8000,
        tilemap_encoding: LevelLayer2TilemapEncoding::Legacy { high_byte: 0 },
    };
    let mut project = Project::new(RomImage::from_bytes(fs::read(&installed).unwrap()).unwrap());
    let loaded = project
        .load_level_layer2_with_descriptor(0x105, 0, layout)
        .unwrap();
    assert_eq!(
        loaded.descriptor,
        Some(MwlLayer2Descriptor::from_raw(0x0c)),
        "installed raw descriptor $08 must normalize like Lunar Magic's legacy tilemap loader"
    );
    let mut expected = loaded.data;
    let NativeLayer2Data::Tilemap(bytes) = &mut expected else {
        panic!("level 105 mode zero must use a compressed Layer 2 tilemap");
    };
    bytes[0] ^= 1;
    let allocation_start = project.rom.logical_len();
    let logical_len = allocation_start + 0x8000;
    project
        .expand_rom(Mapper::LoRom, logical_len, 0xff, 0x7fdc)
        .unwrap();
    let options = LevelLayer2SaveOptions {
        allocation: AllocationPolicy {
            search: allocation_start..logical_len,
            bank_size: Some(0x8000),
            fill_bytes: vec![0xff],
            protected: vec![
                ProtectedRange(0x2e600..0x2ec00),
                ProtectedRange(0x7fc0..0x8000),
            ],
        },
        previous_block: None,
        reuse_identical: true,
        erase_fill: 0xff,
    };
    project
        .save_level_layer2_with_checksum(0x105, 0, &expected, layout, &options, 0x7fdc)
        .unwrap();
    assert_eq!(
        project.load_level_layer2(0x105, 0, layout).unwrap(),
        expected
    );
    fs::write(&edited_rom, project.save_snapshot()).unwrap();

    let output = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&edited_rom))
        .arg(wine_path(&exported_mwl))
        .arg("105")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic export stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let exported = MwlFile::decode(&fs::read(&exported_mwl).unwrap()).unwrap();
    let exported_payload = exported
        .payload_section(MwlSectionKind::Layer2)
        .unwrap()
        .payload;
    assert_eq!(exported_payload, expected.encode_mwl().unwrap());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "requires Wine plus local Lunar Magic 3.63 and retained installed-ROM fixture"]
fn lunar_magic_reexports_rust_object_backed_layer2_edit() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lunar_magic = root.join("lm363/Lunar Magic.exe");
    let installed = root.join("oracle-work/lm363/pristine-us/level-save-105/after.smc");
    let mut project = Project::new(RomImage::from_bytes(fs::read(&installed).unwrap()).unwrap());
    let level_layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let layer2_layout = lm_profile::smw_us_v1_layer2_layout(&project.rom).unwrap();
    let sprite_lengths = lm_level::SpriteLengthTable::standard();
    let (level, level_mode, mut loaded, object_index) = (0..0x200)
        .find_map(|level| {
            let slot = project
                .load_level_slot(level, level_layout, &sprite_lengths)
                .ok()?;
            let level_mode = slot.layer1.header.level_mode();
            let loaded = project
                .load_level_layer2_with_descriptor(level, level_mode, layer2_layout)
                .ok()?;
            let NativeLayer2Data::Objects(objects) = &loaded.data else {
                return None;
            };
            let object_index = objects
                .objects
                .records
                .iter()
                .position(|record| record.command_id() != 0)?;
            Some((level, level_mode, loaded, object_index))
        })
        .expect("installed SMW must contain an object-backed Layer 2 level");
    let NativeLayer2Data::Objects(objects) = &mut loaded.data else {
        unreachable!();
    };
    let placement = objects
        .objects
        .native_placements_for_orientation(objects.header.is_vertical())
        .into_iter()
        .find(|placement| placement.record_index == object_index)
        .expect("the selected ordinary object must have a resolved placement");
    let original = objects.objects.records[object_index].clone();
    let original_coordinates = original.coordinate_nibbles();
    objects
        .objects
        .relocate_ordinary_object_position(
            object_index,
            placement.screen,
            lm_level::ObjectCoordinateNibbles {
                first: original_coordinates.first ^ 1,
                second: original_coordinates.second,
            },
            original.perpendicular_high_coordinate(),
        )
        .unwrap();

    let allocation_start = project.rom.logical_len();
    let logical_len = allocation_start + 0x8000;
    project
        .expand_rom(Mapper::LoRom, logical_len, 0xff, 0x7fdc)
        .unwrap();
    project
        .save_level_layer2_with_descriptor_and_checksum(
            level,
            level_mode,
            &loaded,
            layer2_layout,
            &LevelLayer2SaveOptions {
                allocation: AllocationPolicy {
                    search: allocation_start..logical_len,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![
                        ProtectedRange(0x2e600..0x2ec00),
                        ProtectedRange(0x77310..0x77510),
                        ProtectedRange(0x7fc0..0x8000),
                    ],
                },
                previous_block: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
            0x7fdc,
        )
        .unwrap();
    assert_eq!(
        project
            .load_level_layer2_with_descriptor(level, level_mode, layer2_layout)
            .unwrap(),
        loaded
    );

    let directory = std::env::temp_dir().join(format!(
        "lm-layer2-object-wine-oracle-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let edited_rom = directory.join("Rust Layer 2 object edit.smc");
    let exported_mwl = directory.join("object Layer 2 reexported.mwl");
    fs::write(&edited_rom, project.save_snapshot()).unwrap();
    let output = Command::new("wine")
        .env("WINEDEBUG", "-all")
        .arg(&lunar_magic)
        .arg("-ExportLevel")
        .arg(wine_path(&edited_rom))
        .arg(wine_path(&exported_mwl))
        .arg(format!("{level:03X}"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Lunar Magic export stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let exported = MwlFile::decode(&fs::read(&exported_mwl).unwrap()).unwrap();
    let exported_payload = exported
        .payload_section(MwlSectionKind::Layer2)
        .unwrap()
        .payload;
    assert_eq!(exported_payload, loaded.data.encode_mwl().unwrap());
    fs::remove_dir_all(directory).unwrap();
}
