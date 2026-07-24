use lm_graphics::{CompactExAnimation, CompactExAnimationFile, ExAnimationRecord};
use lm_project::{
    ExAnimationRomLayout, ExAnimationSaveOptions, LevelPointerTable, Project,
    RatsOwnershipManifest, RatsOwnershipManifestFile,
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
const MAXIMUM_RECORDS: usize = 32;
const MODES: [bool; 256] = [false; 256];

fn animation(frame: [u8; 2]) -> CompactExAnimation {
    CompactExAnimation {
        setting: 3,
        header_value: 0x1234_5678,
        trigger_mask: 0xffff,
        trigger_values: std::array::from_fn(|index| index.to_le_bytes()[0]),
        records: vec![ExAnimationRecord::new(1, 0, 2, 0x123, false, &frame, false).unwrap()],
    }
}

fn layout() -> ExAnimationRomLayout {
    ExAnimationRomLayout {
        mapper: Mapper::LoRom,
        pointers: LevelPointerTable {
            offset: POINTER_TABLE,
            entries: 0x200,
            stride: 3,
        },
        maximum_records: MAXIMUM_RECORDS,
        maximum_encoded_len: 0x4000,
    }
}

fn write_fixture(path: &Path, value: &CompactExAnimation) {
    let encoded = value.encode(&MODES).unwrap();
    let mut bytes = vec![0xff; 0x10000];
    let pointer = pc_to_snes(Mapper::LoRom, 0x2000).unwrap().to_le_bytes();
    bytes[POINTER_TABLE + SLOT * 3..POINTER_TABLE + SLOT * 3 + 3].copy_from_slice(&pointer[..3]);
    bytes[0x2000..0x2000 + encoded.len()].copy_from_slice(&encoded);
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
fn built_binary_transfers_interpretation_bound_exanimation() {
    let directory = std::env::temp_dir().join(format!(
        "lm-exanimation-transfer-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input game.smc");
    let modes = directory.join("Size modes.bin");
    let exported = directory.join("Exported animation.lmex");
    let replacement = directory.join("Replacement animation.lmex");
    let output = directory.join("Imported game.smc");
    write_fixture(&input, &animation([7, 8]));
    fs::write(&modes, [0; 256]).unwrap();

    let export = invoke(&[
        "exanimation-export",
        input.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "20",
        "4000",
        modes.to_str().unwrap(),
        exported.to_str().unwrap(),
    ]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    let decoded =
        CompactExAnimationFile::decode(&fs::read(&exported).unwrap(), MAXIMUM_RECORDS, &MODES)
            .unwrap();
    assert_eq!(decoded.source_slot, 1);
    assert_eq!(decoded.animation, animation([7, 8]));

    fs::write(
        &replacement,
        CompactExAnimationFile {
            source_slot: 1,
            animation: animation([9, 10]),
        }
        .encode(&MODES)
        .unwrap(),
    )
    .unwrap();
    let import_arguments = [
        "exanimation-import",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "20",
        "4000",
        modes.to_str().unwrap(),
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
        reopened.load_exanimation(SLOT, layout(), &MODES).unwrap(),
        animation([9, 10])
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    assert!(!invoke(&import_arguments).status.success());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn built_binary_owned_import_reclaims_displaced_exanimation_block() {
    let directory = std::env::temp_dir().join(format!(
        "lm-exanimation-owned-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Owned input.smc");
    let modes = directory.join("Size modes.bin");
    let replacement = directory.join("Owned replacement.lmex");
    let manifest = directory.join("Ownership.lmrats");
    let output = directory.join("Owned output.smc");

    let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x10000]).unwrap());
    let displaced = project
        .save_exanimation(
            SLOT,
            &animation([1, 2]),
            layout(),
            &MODES,
            &ExAnimationSaveOptions {
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
    fs::write(&modes, [0; 256]).unwrap();
    fs::write(
        &replacement,
        CompactExAnimationFile {
            source_slot: u16::try_from(SLOT).unwrap(),
            animation: animation([11, 12]),
        }
        .encode(&MODES)
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
        "exanimation-import-owned",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "1",
        "200",
        "20",
        "4000",
        modes.to_str().unwrap(),
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
        reopened.load_exanimation(SLOT, layout(), &MODES).unwrap(),
        animation([11, 12])
    );
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
