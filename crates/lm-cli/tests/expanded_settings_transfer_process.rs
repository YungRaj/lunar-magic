use lm_level::ExpandedLevelSettingsRecord;
use lm_project::{ExpandedLevelSettingsLayout, Project};
use lm_rom::{Mapper, RomImage, SnesChecksum, compute_snes_checksum};
use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SLOT: usize = 3;

fn layout() -> ExpandedLevelSettingsLayout {
    ExpandedLevelSettingsLayout {
        mapper: Mapper::LoRom,
        table_offset: 0x100,
        entries: 0x200,
        stride: 0x20,
    }
}

fn invoke(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn built_binary_transfers_one_expanded_settings_record_losslessly() {
    let directory = std::env::temp_dir().join(format!(
        "lm-expanded-settings-transfer-日本語-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input game.smc");
    let exported = directory.join("Exported settings.bin");
    let replacement = directory.join("Replacement settings.bin");
    let output = directory.join("Imported game.smc");
    let mut source = vec![0xff; 0x8000];
    source[0x140..0x160].fill(0x11);
    source[0x160..0x180].fill(0x22);
    source[0x180..0x1a0].fill(0x33);
    let checksum = compute_snes_checksum(&source, 0x7fdc).unwrap();
    source[0x7fdc..0x7fe0].copy_from_slice(&checksum.encoded());
    fs::write(&input, &source).unwrap();

    let export = invoke(&[
        "expanded-settings-export",
        input.to_str().unwrap(),
        "lorom",
        "3",
        "100",
        "200",
        "20",
        exported.to_str().unwrap(),
    ]);
    assert!(
        export.status.success(),
        "{}",
        String::from_utf8_lossy(&export.stderr)
    );
    assert_eq!(fs::read(&exported).unwrap(), [0x22; 0x20]);

    fs::write(
        &replacement,
        std::array::from_fn::<_, 32, _>(|index| u8::try_from(index).unwrap().wrapping_mul(7)),
    )
    .unwrap();
    let import_arguments = [
        "expanded-settings-import",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "lorom",
        "3",
        "100",
        "200",
        "20",
        replacement.to_str().unwrap(),
        "7fdc",
    ];
    let import = invoke(&import_arguments);
    assert!(
        import.status.success(),
        "{}",
        String::from_utf8_lossy(&import.stderr)
    );
    let bytes = fs::read(&output).unwrap();
    let reopened = Project::new(RomImage::from_bytes(bytes.clone()).unwrap());
    let expected = ExpandedLevelSettingsRecord::decode(&fs::read(&replacement).unwrap()).unwrap();
    assert_eq!(
        reopened
            .load_expanded_level_settings(SLOT, layout())
            .unwrap(),
        expected
    );
    assert_eq!(&bytes[0x140..0x160], &source[0x140..0x160]);
    assert_eq!(&bytes[0x180..0x1a0], &source[0x180..0x1a0]);
    assert_eq!(
        SnesChecksum::decode(&bytes, 0x7fdc).unwrap(),
        compute_snes_checksum(&bytes, 0x7fdc).unwrap()
    );
    assert!(!invoke(&import_arguments).status.success());
    fs::remove_dir_all(directory).unwrap();
}
