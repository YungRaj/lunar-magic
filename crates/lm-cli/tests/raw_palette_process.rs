use lm_graphics::{Bgr555, Palette, PaletteMaskFile, RawSnesPaletteFile};
use lm_oracle::Observation;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn built_cli_normalizes_and_observes_raw_palette_and_mask() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-raw-palette-process-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let raw_input = directory.join("raw palette 日本語.bin");
    let raw_output = directory.join("normalized palette.bin");
    let raw_observation = directory.join("raw palette.obs");
    let raw = RawSnesPaletteFile {
        palette: Palette {
            colors: (0_u16..=256).map(Bgr555).collect(),
        },
    };
    fs::write(&raw_input, raw.encode().unwrap()).unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("raw-palette-file")
            .arg(&raw_input)
            .arg(&raw_output)
            .arg(&raw_observation)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        RawSnesPaletteFile::decode(&fs::read(&raw_output).unwrap()).unwrap(),
        raw
    );
    let observed = Observation::from_text(&fs::read_to_string(&raw_observation).unwrap()).unwrap();
    assert_eq!(observed.get("raw-palette/color-count"), Some("257"));

    let mask_input = directory.join("selection.palm");
    let mask_output = directory.join("normalized.palm");
    let mask_observation = directory.join("mask.obs");
    let mut mask = vec![0; PaletteMaskFile::FILE_LEN];
    mask[256] = 0x80;
    fs::write(&mask_input, &mask).unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("palette-mask-file")
            .arg(&mask_input)
            .arg(&mask_output)
            .arg(&mask_observation)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&mask_output).unwrap(), mask);
    let observed = Observation::from_text(&fs::read_to_string(&mask_observation).unwrap()).unwrap();
    assert_eq!(observed.get("palette-mask/entries/0100/raw"), Some("128"));
    fs::remove_dir_all(directory).unwrap();
}
