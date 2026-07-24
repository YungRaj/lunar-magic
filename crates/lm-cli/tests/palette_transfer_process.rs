use lm_graphics::{Bgr555, Palette, PaletteInterchangeFile};
use lm_project::{
    LevelPointerTable, PaletteRomLayout, PaletteSaveOptions, Project, RatsOwnershipManifest,
    RatsOwnershipManifestFile,
};
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum, pc_to_snes};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SLOT: usize = 1;
const POINTER_TABLE: usize = 0x200;
const COLORS: usize = 32;

fn palette(seed: u16) -> Palette {
    Palette {
        colors: (0..u16::try_from(COLORS).unwrap())
            .map(|index| Bgr555(seed.wrapping_add(index) & 0x7fff))
            .collect(),
    }
}

fn layout() -> PaletteRomLayout {
    PaletteRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: POINTER_TABLE,
            entries: 0x200,
            stride: 3,
        },
        colors_per_palette: COLORS,
    }
}

fn write_fixture(path: &Path, value: &Palette) {
    let mut bytes = vec![0xff; 0x10000];
    let pointer = pc_to_snes(Mapper::LoRom, 0x2000).unwrap().to_le_bytes();
    bytes[POINTER_TABLE + SLOT * 3..POINTER_TABLE + SLOT * 3 + 3].copy_from_slice(&pointer[..3]);
    bytes[0x2000..0x2000 + COLORS * 2].copy_from_slice(&value.encode_snes().unwrap());
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
fn built_binary_exports_and_imports_palette_with_semantic_reopen() {
    let directory = std::env::temp_dir().join(format!(
        "lm-palette-transfer-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input game.smc");
    let exported = directory.join("Exported palette.lmpal");
    let replacement = directory.join("Replacement palette.lmpal");
    let output = directory.join("Imported game.smc");
    write_fixture(&input, &palette(3));

    let export = invoke(&[
        "palette-export",
        input.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "20",
        exported.to_str().unwrap(),
    ]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let decoded = PaletteInterchangeFile::decode(&fs::read(&exported).unwrap()).unwrap();
    assert_eq!(decoded.source_palette, 1);
    assert_eq!(decoded.palette, palette(3));

    fs::write(
        &replacement,
        PaletteInterchangeFile {
            source_palette: 1,
            palette: palette(0x123),
        }
        .encode()
        .unwrap(),
    )
    .unwrap();
    let import_arguments = [
        "palette-import",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "20",
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
        reopened.load_palette(SLOT, layout()).unwrap(),
        palette(0x123)
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    assert!(!invoke(&import_arguments).status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_binary_owned_import_reclaims_exact_displaced_palette_block() {
    let directory = std::env::temp_dir().join(format!(
        "lm-palette-owned-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Owned input.smc");
    let replacement = directory.join("Owned replacement.lmpal");
    let manifest = directory.join("Ownership.lmrats");
    let output = directory.join("Owned output.smc");

    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let displaced = project
        .save_palette(
            SLOT,
            &palette(9),
            layout(),
            &PaletteSaveOptions {
                allocation: AllocationPolicy {
                    search: 0x1000..0x3000,
                    bank_size: Some(0x8000),
                    fill_bytes: vec![0xff],
                    protected: vec![ProtectedRange(POINTER_TABLE..POINTER_TABLE + 0x600)],
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
        PaletteInterchangeFile {
            source_palette: u16::try_from(SLOT).unwrap(),
            palette: palette(0x321),
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
        "palette-import-owned",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "20",
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
        reopened.load_palette(SLOT, layout()).unwrap(),
        palette(0x321)
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
