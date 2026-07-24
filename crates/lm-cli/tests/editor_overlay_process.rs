use lm_oracle::Observation;
use lm_render::{EditorOverlay, EditorOverlayFile, Rgba, SelectionOverlay, WorldRect};
use std::{fs, process::Command};

#[test]
fn built_cli_normalizes_and_observes_overlay_artifacts() {
    let directory = std::env::temp_dir().join(format!(
        "lm-cli-editor-overlay-process-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let input = directory.join("Input Overlays 日本語.lmovly");
    let normalized = directory.join("Normalized Overlays.lmovly");
    let observation = directory.join("Overlay Observation.obs");
    let file = EditorOverlayFile {
        overlays: vec![EditorOverlay::Selection(SelectionOverlay {
            bounds: WorldRect {
                left: -2,
                top: 3,
                right: 10,
                bottom: 12,
            },
            light: Rgba {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
            dark: Rgba {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            dash_length: 2,
            phase: 7,
        })],
    };
    fs::write(&input, file.encode().unwrap()).unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("editor-overlay-file")
        .arg(&input)
        .arg(&normalized)
        .arg(&observation)
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(
        EditorOverlayFile::decode(&fs::read(&normalized).unwrap()).unwrap(),
        file
    );
    let observed = Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
    assert_eq!(observed.get("editor-overlays/0000/left"), Some("-2"));
    assert_eq!(observed.get("editor-overlays/0000/phase"), Some("7"));

    let second = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .arg("editor-overlay-file")
        .arg(&input)
        .arg(&normalized)
        .arg(&observation)
        .output()
        .unwrap();
    assert!(!second.status.success());
    assert_eq!(fs::read(&normalized).unwrap(), file.encode().unwrap());
    fs::remove_dir_all(directory).unwrap();
}
