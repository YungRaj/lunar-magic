use lm_profile::{
    SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START, SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
    SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT, smw_us_v1_default_special_expanded_settings_record,
};
use lm_project::{ExpandedLevelSettingsLayout, Project};
use lm_rats::parse_at;
use lm_rom::{Mapper, RomImage, detect_identity};
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn built_cli_installs_reopens_and_refuses_replacement() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!(
        "lm expanded settings 日本語 {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("pristine source.smc");
    let output = directory.join("expanded settings.smc");
    let original = fs::read(
        root.join("oracle-work/lm363/pristine-us/mwl-layer3-settings-positive/before.smc"),
    )
    .unwrap();
    fs::write(&input, &original).unwrap();

    let run = || {
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("expanded-settings-install")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(fs::read(&input).unwrap(), original);

    let image = RomImage::from_bytes(fs::read(&output).unwrap()).unwrap();
    assert!(detect_identity(&image).unwrap().checksum_matches());
    let block = parse_at(
        image.logical_bytes(),
        SMW_US_V1_EXPANDED_SETTINGS_ALLOCATION_SEARCH_START,
    )
    .unwrap();
    let project = Project::open_supported(image).unwrap();
    let layout = ExpandedLevelSettingsLayout {
        mapper: Mapper::LoRom,
        table_offset: block.payload.start + SMW_US_V1_EXPANDED_SETTINGS_PREFIX_LEN,
        entries: SMW_US_V1_EXPANDED_SETTINGS_RECORD_COUNT,
        stride: 0x20,
    };
    assert_eq!(
        project.load_expanded_level_settings(0x206, layout).unwrap(),
        smw_us_v1_default_special_expanded_settings_record()
    );

    let second = run();
    assert!(!second.status.success());
    fs::remove_dir_all(directory).unwrap();
}
