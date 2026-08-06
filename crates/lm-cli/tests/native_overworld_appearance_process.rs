use lm_level::S16OvSidecar;
use lm_oracle::Observation;
use lm_overworld::NativeOverworldSpriteSidecar;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn built_cli_round_trips_native_pair_through_unicode_paths() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-native-overworld-appearance-日本語-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let definitions = directory.join("Sprites 日本語.sscov");
    let map16 = directory.join("Sprites 日本語.s16ov");
    let normalized_definitions = directory.join("Normalized 日本語.sscov");
    let normalized_map16 = directory.join("Normalized 日本語.s16ov");
    let observation = directory.join("Sprites 日本語.obs");
    fs::write(
        &definitions,
        b"\xEF\xBB\xBF05\t1\tTooltip\r\n05\t3\t-2,4,8400 8,-9,C01\r\n10000\t12\t400-4FF,1234\r\n",
    )
    .unwrap();
    fs::write(&map16, [1, 0, 0, 0, 2]).unwrap();

    let process = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("native-overworld-appearance-file")
        .arg(&definitions)
        .arg(&map16)
        .arg(&normalized_definitions)
        .arg(&normalized_map16)
        .arg(&observation)
        .output()
        .unwrap();
    assert!(
        process.status.success(),
        "{}",
        String::from_utf8_lossy(&process.stderr)
    );
    let decoded =
        NativeOverworldSpriteSidecar::decode(&fs::read(normalized_definitions).unwrap()).unwrap();
    assert!(decoded.appearances[&5].shadow);
    assert_eq!(
        S16OvSidecar::decode(&fs::read(normalized_map16).unwrap())
            .unwrap()
            .loaded_len(),
        5
    );
    let observed = Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
    assert_eq!(
        observed.get("native-overworld-appearances/sprites/005/appearance/parts/0000/translucent"),
        Some("true")
    );
    assert_eq!(
        observed.get("native-overworld-appearances/graphics-ranges/0000/base"),
        Some("1234")
    );
    fs::remove_dir_all(directory).unwrap();
}
