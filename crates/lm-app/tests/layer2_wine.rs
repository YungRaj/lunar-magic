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
