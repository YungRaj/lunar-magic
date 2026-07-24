use lm_level::{Map16Page, Map16PageFile, Map16Tile, Subtile};
use lm_project::{
    LevelPointerTable, Map16RomLayout, Map16SaveOptions, Project, RatsOwnershipManifest,
    RatsOwnershipManifestFile,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum, pc_to_snes};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const PAGE: usize = 1;
const GRAPHICS_TABLE: usize = 0x200;
const ACTS_LIKE_TABLE: usize = 0x500;

fn page(seed: u16) -> Map16Page {
    let mut tiles = vec![Map16Tile::default(); Map16Page::TILE_COUNT];
    tiles[3] = Map16Tile {
        top_left: Subtile(seed),
        top_right: Subtile(seed.wrapping_add(1)),
        bottom_left: Subtile(seed.wrapping_add(2)),
        bottom_right: Subtile(seed.wrapping_add(3)),
        acts_like: seed & 0x1ff,
    };
    Map16Page::new(tiles).unwrap()
}

fn layout() -> Map16RomLayout {
    let pointers = |offset| LevelPointerTable {
        offset,
        entries: 0x100,
        stride: 3,
    };
    Map16RomLayout {
        mapper: Mapper::LoRom,
        graphics: pointers(GRAPHICS_TABLE),
        acts_like: pointers(ACTS_LIKE_TABLE),
    }
}

fn write_fixture(path: &Path, value: &Map16Page) {
    let mut bytes = vec![0xff; 0x10000];
    for (table, target) in [(GRAPHICS_TABLE, 0x1000), (ACTS_LIKE_TABLE, 0x2000)] {
        let pointer = pc_to_snes(Mapper::LoRom, target).unwrap().to_le_bytes();
        bytes[table + PAGE * 3..table + PAGE * 3 + 3].copy_from_slice(&pointer[..3]);
    }
    let (graphics, acts_like) = value.encode().unwrap();
    bytes[0x1000..0x1800].copy_from_slice(&graphics);
    bytes[0x2000..0x2200].copy_from_slice(&acts_like);
    let checksum = compute_snes_checksum(&bytes, 0x7fdc).unwrap();
    bytes[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    fs::write(path, bytes).unwrap();
}

fn invoke(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn built_binary_exports_and_atomically_imports_both_map16_planes() {
    let directory = std::env::temp_dir().join(format!(
        "lm-map16-transfer-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input game.smc");
    let exported = directory.join("Exported page.map16");
    let replacement = directory.join("Replacement page.map16");
    let output = directory.join("Imported game.smc");
    write_fixture(&input, &page(0x20));

    let export = invoke(&[
        "map16-export",
        input.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "500",
        exported.to_str().unwrap(),
    ]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let decoded = Map16PageFile::decode(&fs::read(&exported).unwrap()).unwrap();
    assert_eq!(decoded.source_page, 1);
    assert_eq!(decoded.page, page(0x20));

    fs::write(
        &replacement,
        Map16PageFile {
            source_page: 1,
            page: page(0x321),
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    let import_arguments = [
        "map16-import",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "500",
        replacement.to_str().unwrap(),
        "7fdc",
        "3000",
        "7800",
    ];
    let import = invoke(&import_arguments);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    assert_eq!(
        reopened.load_map16_page(PAGE, layout()).unwrap(),
        page(0x321)
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    assert!(!invoke(&import_arguments).status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_binary_owned_import_reclaims_both_displaced_map16_blocks() {
    let directory = std::env::temp_dir().join(format!(
        "lm-map16-owned-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Owned input.smc");
    let replacement = directory.join("Owned replacement.map16");
    let manifest = directory.join("Ownership.lmrats");
    let output = directory.join("Owned output.smc");

    let allocation = AllocationPolicy {
        search: 0x1000..0x3800,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(GRAPHICS_TABLE..GRAPHICS_TABLE + 0x300),
            ProtectedRange(ACTS_LIKE_TABLE..ACTS_LIKE_TABLE + 0x300),
        ],
    };
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let displaced = project
        .save_map16_page(
            PAGE,
            &page(0x40),
            layout(),
            &Map16SaveOptions {
                graphics_allocation: allocation.clone(),
                acts_like_allocation: allocation,
                previous_graphics: None,
                previous_acts_like: None,
                reuse_identical: false,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    let displaced_blocks = vec![displaced.graphics.block, displaced.acts_like.block];
    project.refresh_checksum(0x7fdc).unwrap();
    fs::write(&input, project.save_snapshot()).unwrap();
    fs::write(
        &replacement,
        Map16PageFile {
            source_page: u16::try_from(PAGE).unwrap(),
            page: page(0x456),
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &manifest,
        RatsOwnershipManifestFile(RatsOwnershipManifest {
            owned: displaced_blocks.clone(),
            retained: Vec::new(),
        })
        .encode()
        .unwrap(),
    )
    .unwrap();

    let import = invoke(&[
        "map16-import-owned",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "500",
        replacement.to_str().unwrap(),
        "7fdc",
        "4000",
        "7000",
        manifest.to_str().unwrap(),
    ]);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    for block in &displaced_blocks {
        assert!(bytes[block.full_range()].iter().all(|byte| *byte == 0xff));
    }
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    assert_eq!(
        reopened.load_map16_page(PAGE, layout()).unwrap(),
        page(0x456)
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
