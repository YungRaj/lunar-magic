use lm_oracle::Observation;
use lm_overworld::ExpandedLayerTilemap;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn built_cli_normalizes_and_observes_wine_title_transfer_artifact() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("oracle-work/lm363/pristine-us/title-screen-transfer-positive");
    let directory = std::env::temp_dir().join(format!(
        "lm-cli-layer-tilemap-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let normalized = directory.join("Normalized Title 日本語.lmtile");
    let observation = directory.join("Title Observation.obs");
    let input = fixture.join("after.lmtile");
    let result = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("layer-tilemap-file")
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
    let expected = ExpandedLayerTilemap::decode_native_file(&fs::read(&input).unwrap()).unwrap();
    assert_eq!(
        ExpandedLayerTilemap::decode_native_file(&fs::read(&normalized).unwrap()).unwrap(),
        expected
    );
    let observed = Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
    assert_eq!(
        observed.get("scene/layer-tilemap/secondary-blank"),
        Some("true")
    );
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
