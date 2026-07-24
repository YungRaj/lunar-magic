use lm_graphics::{GraphicsFile4bpp, GraphicsInterchangeFile, IndexedTile};
use lm_project::{
    GraphicsCompression, GraphicsRomLayout, GraphicsSaveOptions, LevelPointerTable, Project,
    RatsOwnershipManifest, RatsOwnershipManifestFile,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn layout(compression: GraphicsCompression) -> GraphicsRomLayout {
    GraphicsRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: 0x200,
            entries: 0x100,
            stride: 3,
        },
        compression,
        maximum_compressed_len: 0x8000,
        maximum_decompressed_len: 0x10000,
    }
}

fn write_graphics_rom(path: &std::path::Path, value: u8) {
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    project
        .save_graphics_file(
            0,
            &graphics(value),
            layout(GraphicsCompression::Lz2),
            &GraphicsSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x1000..0x7000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(0x200..0x500)],
                },
                previous_block: None,
                reuse_identical: false,
                erase_fill: 0xff,
            },
        )
        .unwrap();
    project.refresh_checksum(0x7fdc).unwrap();
    fs::write(path, project.save_snapshot()).unwrap();
}

fn invoke(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn built_binary_exports_and_imports_graphics_with_semantic_reopen() {
    let directory = std::env::temp_dir().join(format!(
        "lm-cli-graphics-transfer-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input game.smc");
    let exported = directory.join("Exported graphics.lmgfx");
    let replacement = directory.join("Replacement graphics.lmgfx");
    let output = directory.join("Imported game.smc");
    write_graphics_rom(&input, 2);

    let export = invoke(&[
        "graphics-export",
        input.to_str().unwrap(),
        "lorom",
        "0",
        "200",
        "8000",
        "10000",
        "lz2",
        exported.to_str().unwrap(),
    ]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let decoded = GraphicsInterchangeFile::decode(&fs::read(&exported).unwrap()).unwrap();
    assert_eq!(decoded.source_slot, 0);
    assert_eq!(decoded.graphics, graphics(2));

    fs::write(
        &replacement,
        GraphicsInterchangeFile {
            source_slot: 0,
            graphics: graphics(13),
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    let import_arguments = [
        "graphics-import",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "0",
        "200",
        "8000",
        "10000",
        "lz2",
        replacement.to_str().unwrap(),
        "7fdc",
        "1000",
        "7000",
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
        reopened
            .load_graphics_file(0, layout(GraphicsCompression::Lz2))
            .unwrap(),
        graphics(13)
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    assert!(!invoke(&import_arguments).status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_binary_owned_import_reclaims_exact_displaced_graphics_block() {
    let directory = std::env::temp_dir().join(format!(
        "lm-cli-graphics-owned-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Owned input.smc");
    let replacement = directory.join("Owned replacement.lmgfx");
    let manifest = directory.join("Ownership.lmrats");
    let output = directory.join("Owned output.smc");

    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let displaced = project
        .save_graphics_file(
            0,
            &graphics(3),
            layout(GraphicsCompression::Lz2),
            &GraphicsSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x1000..0x3000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(0x200..0x500)],
                },
                previous_block: None,
                reuse_identical: false,
                erase_fill: 0xff,
            },
        )
        .unwrap()
        .block;
    project.refresh_checksum(0x7fdc).unwrap();
    fs::write(&input, project.save_snapshot()).unwrap();
    fs::write(
        &replacement,
        GraphicsInterchangeFile {
            source_slot: 0,
            graphics: graphics(14),
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &manifest,
        RatsOwnershipManifestFile(RatsOwnershipManifest {
            owned: vec![displaced.clone()],
            retained: Vec::new(),
        })
        .encode()
        .unwrap(),
    )
    .unwrap();

    let import = invoke(&[
        "graphics-import-owned",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "0",
        "200",
        "8000",
        "10000",
        "lz2",
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
    assert!(
        bytes[displaced.full_range()]
            .iter()
            .all(|byte| *byte == 0xff)
    );
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    assert_eq!(
        reopened
            .load_graphics_file(0, layout(GraphicsCompression::Lz2))
            .unwrap(),
        graphics(14)
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}

fn graphics(value: u8) -> GraphicsFile4bpp {
    GraphicsFile4bpp {
        tiles: vec![IndexedTile::new([value; IndexedTile::PIXEL_COUNT])],
    }
}

#[test]
fn built_binary_atomically_recompresses_a_unicode_path_fixture() {
    let directory = std::env::temp_dir().join(format!(
        "lm-cli-recompress-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input graphics.smc");
    let output = directory.join("Output graphics.smc");
    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    for (slot, value) in [2, 11].into_iter().enumerate() {
        project
            .save_graphics_file(
                slot,
                &graphics(value),
                layout(GraphicsCompression::Lz2),
                &GraphicsSaveOptions {
                    allocation: AllocationPolicy {
                        search: 0x1000..0x7000,
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0xff],
                        protected: vec![ProtectedRange(0x200..0x206)],
                    },
                    previous_block: None,
                    reuse_identical: false,
                    erase_fill: 0xff,
                },
            )
            .unwrap();
    }
    project.refresh_checksum(0x7fdc).unwrap();
    fs::write(&input, project.save_snapshot()).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args([
            "graphics-recompress",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "lorom",
            "200",
            "2",
            "8000",
            "10000",
            "lz2",
            "lz3",
            "7fdc",
            "1000",
            "7000",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    for (slot, value) in [2, 11].into_iter().enumerate() {
        assert_eq!(
            reopened
                .load_graphics_file(slot, layout(GraphicsCompression::Lz3))
                .unwrap(),
            graphics(value)
        );
    }
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    let collision = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args([
            "graphics-recompress",
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            "lorom",
            "200",
            "2",
            "8000",
            "10000",
            "lz2",
            "lz3",
            "7fdc",
            "1000",
            "7000",
        ])
        .output()
        .unwrap();
    assert!(!collision.status.success());
    fs::remove_dir_all(directory).unwrap();
}
