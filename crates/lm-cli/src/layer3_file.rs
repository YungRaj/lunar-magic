use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_level::Layer3File;
use lm_oracle::observe_layer3;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    for output in [normalized_output, observation].into_iter().flatten() {
        if output == input {
            return Err("Layer 3 outputs must differ from input".into());
        }
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized Layer 3 and observation outputs must differ".into());
    }
    let file = Layer3File::decode(&read_bounded(input, Layer3File::MAX_ENCODED_LEN)?)?;
    println!("tilemap-bytes: {}", file.0.tilemap.len());
    println!("remap-command-bytes: {}", file.0.remap_commands.len());
    println!(
        "graphics: {:03X} {:03X} {:03X} {:03X}",
        file.0.settings.graphics_files[0],
        file.0.settings.graphics_files[1],
        file.0.settings.graphics_files[2],
        file.0.settings.graphics_files[3]
    );
    let normalized = normalized_output.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_layer3(&file.0).to_text());
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized_output, normalized.as_ref()) {
        outputs.push((path, bytes.as_slice()));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Layer3Data, Layer3Settings};

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("lm-cli-layer3-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn normalization_cannot_overwrite_input() {
        let input = Path::new("same.lmlayer3");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, None, Some(input)).is_err());
        let output = Path::new("same-output");
        assert!(execute(input, Some(output), Some(output)).is_err());
    }

    #[test]
    fn normalization_and_observation_publish_together() {
        let directory = directory();
        let input = directory.join("input.lmlayer3");
        let normalized = directory.join("normalized.lmlayer3");
        let observation = directory.join("layer3.obs");
        let file = Layer3File(Layer3Data {
            settings: Layer3Settings {
                start_position: 2,
                graphics_files: [1, 2, 3, 4],
                ..Layer3Settings::default()
            },
            tilemap: vec![1, 2, 3],
            remap_commands: vec![0x80, 0xff],
        });
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            Layer3File::decode(&fs::read(&normalized).unwrap()).unwrap(),
            file
        );
        let observed = lm_oracle::Observation::from_text(
            std::str::from_utf8(&fs::read(&observation).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(observed.get("layer3/start-position"), Some("2"));
        assert_eq!(observed.get("layer3/remap-commands"), Some("80ff"));
        fs::remove_dir_all(directory).unwrap();
    }
}
