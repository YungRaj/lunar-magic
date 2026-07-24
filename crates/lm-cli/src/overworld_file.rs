use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_complete_overworld;
use lm_project::CompleteOverworldFile;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    size_modes: &Path,
    maximum_animation_records: usize,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, size_modes, normalized, observation)?;
    let modes = crate::size_mode_file::read(size_modes)?;
    let file = CompleteOverworldFile::decode(
        &read_bounded(input, CompleteOverworldFile::MAX_FILE_LEN)?,
        maximum_animation_records,
        &modes,
    )?;
    println!("source-slot: {}", file.source_slot);
    println!("dimensions: {}x{}", file.shape.width, file.shape.height);
    println!("event-reveals: {}", file.data.event_reveals.entries.len());
    let encoded = normalized.map(|_| file.encode(&modes)).transpose()?;
    let observed = observation.map(|_| observe_complete_overworld(&file).to_text());
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized, encoded.as_ref()) {
        outputs.push((path, bytes.as_slice()));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

fn validate_paths(
    input: &Path,
    size_modes: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), &'static str> {
    if input == size_modes {
        return Err("overworld input and size-mode table must differ");
    }
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|output| output == input || output == size_modes)
    {
        return Err("overworld outputs must differ from every input");
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized overworld and observation outputs must differ");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, Palette};
    use lm_overworld::{EventRevealTable, OverworldLayer};
    use lm_project::{CompleteOverworldData, CompleteOverworldShape, OverworldLayers};

    fn file() -> CompleteOverworldFile {
        CompleteOverworldFile {
            source_slot: 5,
            shape: CompleteOverworldShape {
                width: 1,
                height: 1,
                event_reveals: 0,
                endpoints: 0,
                messages: 0,
                sprites: 0,
                sprite_record_len: 7,
                palette_colors: 1,
            },
            data: CompleteOverworldData {
                layers: OverworldLayers {
                    layer1: OverworldLayer::new(1, 1, vec![0x123]).unwrap(),
                    layer2: OverworldLayer::new(1, 1, vec![0x456]).unwrap(),
                },
                event_reveals: EventRevealTable::default(),
                endpoints: Vec::new(),
                messages: Vec::new(),
                sprites: Vec::new(),
                palette: Palette {
                    colors: vec![Bgr555(0x1f)],
                },
                animation: CompactExAnimation {
                    setting: 0,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: Vec::new(),
                },
            },
        }
    }

    #[test]
    fn aliases_fail_before_file_access() {
        let input = Path::new("input");
        let modes = Path::new("modes");
        let output = Path::new("output");
        assert!(execute(input, input, 32, None, None).is_err());
        assert!(execute(input, modes, 32, Some(input), None).is_err());
        assert!(execute(input, modes, 32, Some(output), Some(output)).is_err());
    }

    #[test]
    fn all_domains_normalize_and_observe_together() {
        let directory =
            std::env::temp_dir().join(format!("lm-cli-overworld-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("input.lmow");
        let modes_path = directory.join("modes.bin");
        let normalized = directory.join("normalized.lmow");
        let observation = directory.join("world.obs");
        let modes = [false; 256];
        let expected = file();
        fs::write(&input, expected.encode(&modes).unwrap()).unwrap();
        fs::write(&modes_path, [0; 256]).unwrap();
        execute(
            &input,
            &modes_path,
            32,
            Some(&normalized),
            Some(&observation),
        )
        .unwrap();
        assert_eq!(
            CompleteOverworldFile::decode(&fs::read(&normalized).unwrap(), 32, &modes).unwrap(),
            expected
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
        assert_eq!(observed.get("overworld/layer1/tiles/000000"), Some("291"));
        assert_eq!(
            observed.get("overworld/palette/colors/0000/bgr555"),
            Some("31")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
