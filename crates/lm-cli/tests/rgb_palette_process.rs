use lm_graphics::{Bgr555, Palette, RgbChannelExpansion, RgbPaletteFile};
use lm_oracle::Observation;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn built_cli_preserves_and_observes_rgb_palette_expansion() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-rgb-palette-process-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("palette 日本語.pal");
    let normalized = directory.join("normalized palette.pal");
    let observation = directory.join("palette observation.txt");
    let file = RgbPaletteFile::from_snes_palette(
        &Palette {
            colors: (0_u16..256).map(Bgr555).collect(),
        },
        RgbChannelExpansion::ReplicatedBits,
    )
    .unwrap();
    let bytes = file.encode().unwrap();
    fs::write(&input, &bytes).unwrap();

    assert!(
        Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .arg("rgb-palette-file")
            .arg(&input)
            .arg(&normalized)
            .arg(&observation)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(fs::read(&normalized).unwrap(), bytes);
    let observed = Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
    assert_eq!(
        observed.get("rgb-palette/expansion"),
        Some("replicated-bits")
    );
    assert_eq!(observed.get("rgb-palette/color-count"), Some("256"));
    fs::remove_dir_all(directory).unwrap();
}
