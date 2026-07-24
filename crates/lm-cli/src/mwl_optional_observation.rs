use crate::{atomic_output::write_new, oracle_input::read_bounded, size_mode_file};
use lm_level::MwlFile;
use lm_project::MwlOptionalLevelAssets;
use std::path::Path;

pub fn execute(
    input: &Path,
    size_modes: &Path,
    maximum_records: usize,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == output || size_modes == output {
        return Err("MWL input, size-mode input, and observation output must differ".into());
    }
    let file = MwlFile::decode(&read_bounded(input, MwlFile::MAX_FILE_BYTES)?)?;
    let modes = size_mode_file::read(size_modes)?;
    let assets = MwlOptionalLevelAssets::decode(&file, maximum_records, &modes)?;
    let observation = lm_oracle::observe_mwl_optional_assets(&assets, &modes)?;
    write_new(output, observation.to_text())?;
    println!("observed-mwl-optional-assets: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-mwl-optional-observation-{}-{}-{name}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn emits_field_addressable_semantics_and_is_create_new() {
        let input = path("input.mwl");
        let modes = path("modes.bin");
        let output = path("output.obs");
        let mut file = MwlFile::default();
        MwlOptionalLevelAssets {
            palette_metadata: [1, 2],
            palette: Palette {
                colors: (0_u16..257).map(Bgr555).collect(),
            },
            exanimation_metadata: [3, 4],
            exanimation: None,
        }
        .install_into(&mut file, &[false; 256])
        .unwrap();
        fs::write(&input, file.encode().unwrap()).unwrap();
        fs::write(&modes, [0; 256]).unwrap();

        execute(&input, &modes, 32, &output).unwrap();
        let observation =
            lm_oracle::Observation::from_text(&fs::read_to_string(&output).unwrap()).unwrap();
        assert_eq!(
            observation.get("mwl/optional-assets/palette/colors/0100/bgr555"),
            Some("256")
        );
        assert!(execute(&input, &modes, 32, &output).is_err());

        fs::remove_file(input).unwrap();
        fs::remove_file(modes).unwrap();
        fs::remove_file(output).unwrap();
    }
}
