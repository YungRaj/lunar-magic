use lm_level::S16Sidecar;
use lm_oracle::Observation;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn built_cli_canonicalizes_and_observes_s16_block_rounding() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("lm-s16-process-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Sprites 日本語.s16");
    let normalized = directory.join("Normalized sprites.s16");
    let observation = directory.join("Sprites.obs");
    let mut bytes = vec![0; 0x805];
    bytes[0x800..0x804].copy_from_slice(&0x4433_2211_u32.to_le_bytes());
    fs::write(&input, &bytes).unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args(["native-map16-sidecar", "s16"])
            .arg(&input)
            .arg(&normalized)
            .arg(&observation)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::metadata(&normalized).unwrap().len(), 0x1000);
    let decoded = S16Sidecar::decode(&fs::read(&normalized).unwrap()).unwrap();
    assert_eq!(decoded.entry(0x200), Some(0x4433_2211));
    let observed = Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
    assert_eq!(observed.get("s16/loaded-length"), Some("2053"));
    assert_eq!(observed.get("s16/canonical-length"), Some("4096"));
    assert_eq!(observed.get("s16/entries/0200"), Some("44332211"));
    fs::remove_dir_all(directory).unwrap();
}
