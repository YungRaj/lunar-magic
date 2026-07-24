use lm_graphics::{Bgr555, Palette, TplPaletteFile};
use lm_oracle::Observation;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn built_cli_normalizes_and_observes_version_two_tpl() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "lm-tpl-palette-process-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let input = directory.join("palette 日本語.tpl");
    let normalized = directory.join("normalized palette.tpl");
    let observation = directory.join("palette observation.txt");
    let file = TplPaletteFile {
        palette: Palette {
            colors: (0_u16..256).map(Bgr555).collect(),
        },
    };
    fs::write(&input, file.encode().unwrap()).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("tpl-palette-file")
        .arg(&input)
        .arg(&normalized)
        .arg(&observation)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        TplPaletteFile::decode(&fs::read(&normalized).unwrap()).unwrap(),
        file
    );
    let observed = Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
    assert_eq!(observed.get("tpl-palette/version"), Some("2"));
    assert_eq!(observed.get("tpl-palette/colors/00ff/bgr555"), Some("255"));
    fs::remove_dir_all(directory).unwrap();
}
