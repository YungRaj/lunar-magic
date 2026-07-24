use lm_oracle::Observation;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn built_cli_normalizes_and_observes_grouped_custom_sprites() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-custom-sprite-process-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let data = directory.join("placements 日本語.mw2");
    let text = directory.join("descriptions 日本語.mwt");
    let lengths = directory.join("sprite lengths.bin");
    let output_data = directory.join("normalized placements.mw2");
    let output_text = directory.join("normalized descriptions.mwt");
    let observation = directory.join("sprite placements.obs");
    let data_bytes = [0x5a, 1, 2, 3, 0, 4, 5, 5, 6, 7, 0xff];
    let text_bytes = b"\xef\xbb\xbfPair\r\nSingle \xe2\x98\x83\r\n";
    fs::write(&data, data_bytes).unwrap();
    fs::write(&text, text_bytes).unwrap();
    fs::write(&lengths, [3; 1024]).unwrap();

    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("custom-sprite-library")
            .args([
                &data,
                &text,
                &lengths,
                &output_data,
                &output_text,
                &observation
            ])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(output_data).unwrap(), data_bytes);
    assert_eq!(fs::read(output_text).unwrap(), text_bytes);
    let observed = Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
    assert_eq!(observed.get("custom-sprites/header"), Some("5a"));
    assert_eq!(observed.get("custom-sprites/count"), Some("2"));
    assert_eq!(
        observed.get("custom-sprites/entries/0000/sprite-count"),
        Some("2")
    );
    assert_eq!(
        observed.get("custom-sprites/entries/0001/description"),
        Some("Single ☃")
    );
    fs::remove_dir_all(directory).unwrap();
}
