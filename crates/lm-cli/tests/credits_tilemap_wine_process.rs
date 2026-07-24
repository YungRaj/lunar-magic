use lm_oracle::Observation;
use lm_overworld::CreditsTilemap;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn built_cli_replays_lunar_magic_credits_transfer_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("oracle-work/lm363/pristine-us/credits-transfer-positive");
    let directory =
        std::env::temp_dir().join(format!("lm-cli-credits-wine-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let normalized = directory.join("Normalized Credits 日本語.lmcred");
    let observation = directory.join("Credits Observation.obs");
    let input = fixture.join("after.lmcred");
    let result = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("credits-tilemap-file")
        .arg(&input)
        .arg(&normalized)
        .arg(&observation)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected = CreditsTilemap::decode_native_file(&fs::read(&input).unwrap()).unwrap();
    assert_eq!(
        CreditsTilemap::decode_native_file(&fs::read(&normalized).unwrap()).unwrap(),
        expected
    );
    let observed = Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
    assert!(observed.get("credits/tilemap/row/255/sha256").is_some());
    assert_eq!(
        fs::read_to_string(&observation).unwrap(),
        fs::read_to_string(fixture.join("after.obs")).unwrap()
    );
    let verification = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("oracle-verify")
        .arg(fixture.join("oracle.manifest"))
        .arg(fixture.join("before.smc"))
        .arg(fixture.join("after.smc"))
        .arg(fixture.join("before.obs"))
        .arg(fixture.join("after.obs"))
        .output()
        .unwrap();
    assert!(
        verification.status.success(),
        "{}",
        String::from_utf8_lossy(&verification.stderr)
    );
    fs::remove_dir_all(directory).unwrap();
}
