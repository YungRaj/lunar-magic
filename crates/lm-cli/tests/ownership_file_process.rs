use lm_app::{GraphicsOwnershipFile, PaletteOwnershipFile};
use lm_graphics::{GraphicsOwnership, GraphicsTileOwner, PaletteEntryOwner, PaletteOwnership};
use lm_oracle::Observation;
use std::{fs, process::Command};

#[test]
fn built_cli_normalizes_and_observes_both_ownership_domains_atomically() {
    let directory =
        std::env::temp_dir().join(format!("lm-cli-ownership-process-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir(&directory).unwrap();
    let graphics_input = directory.join("Graphics ownership.lmgfxown");
    let graphics_output = directory.join("Graphics normalized.lmgfxown");
    let graphics_observation = directory.join("Graphics ownership.obs");
    let palette_input = directory.join("Palette ownership.lmpalown");
    let palette_output = directory.join("Palette normalized.lmpalown");
    let palette_observation = directory.join("Palette ownership.obs");

    let graphics = GraphicsOwnershipFile {
        ownership: GraphicsOwnership::from_owners(vec![
            GraphicsTileOwner::Editable,
            GraphicsTileOwner::Fixed,
            GraphicsTileOwner::ExAnimation { record: 7 },
        ]),
    };
    let palette = PaletteOwnershipFile {
        ownership: PaletteOwnership::from_owners(vec![
            PaletteEntryOwner::Fixed,
            PaletteEntryOwner::ExAnimation { record: 9 },
            PaletteEntryOwner::Editable,
        ]),
    };
    fs::write(&graphics_input, graphics.encode().unwrap()).unwrap();
    fs::write(&palette_input, palette.encode().unwrap()).unwrap();

    for (kind, input, output, observation) in [
        (
            "graphics-ownership-file",
            &graphics_input,
            &graphics_output,
            &graphics_observation,
        ),
        (
            "palette-ownership-file",
            &palette_input,
            &palette_output,
            &palette_observation,
        ),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
            .args([kind])
            .arg(input)
            .arg(output)
            .arg(observation)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    assert_eq!(
        GraphicsOwnershipFile::decode(&fs::read(&graphics_output).unwrap()).unwrap(),
        graphics
    );
    assert_eq!(
        PaletteOwnershipFile::decode(&fs::read(&palette_output).unwrap()).unwrap(),
        palette
    );
    let graphics_observed =
        Observation::from_text(&fs::read_to_string(&graphics_observation).unwrap()).unwrap();
    assert_eq!(graphics_observed.get("ownership/domain"), Some("graphics"));
    assert_eq!(
        graphics_observed.get("ownership/entries/0002/record"),
        Some("7")
    );
    let palette_observed =
        Observation::from_text(&fs::read_to_string(&palette_observation).unwrap()).unwrap();
    assert_eq!(palette_observed.get("ownership/domain"), Some("palette"));
    assert_eq!(
        palette_observed.get("ownership/entries/0001/owner"),
        Some("exanimation")
    );

    let before = fs::read(&graphics_output).unwrap();
    let repeated = Command::new(env!("CARGO_BIN_EXE_lm-cli"))
        .args(["graphics-ownership-file"])
        .arg(&graphics_input)
        .arg(&graphics_output)
        .arg(directory.join("Unused observation.obs"))
        .output()
        .unwrap();
    assert!(!repeated.status.success());
    assert_eq!(fs::read(&graphics_output).unwrap(), before);
    assert!(!directory.join("Unused observation.obs").exists());
    fs::remove_dir_all(directory).unwrap();
}
