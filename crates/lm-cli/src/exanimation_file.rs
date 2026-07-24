use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_graphics::CompactExAnimationFile;
use lm_oracle::observe_compact_exanimation;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    size_modes: &Path,
    maximum_records: usize,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, size_modes, normalized, observation)?;
    let modes = crate::size_mode_file::read(size_modes)?;
    let file = CompactExAnimationFile::decode(
        &read_bounded(input, CompactExAnimationFile::MAX_FILE_LEN)?,
        maximum_records,
        &modes,
    )?;
    println!("source-slot: {}", file.source_slot);
    println!("records: {}", file.animation.records.len());
    let encoded = normalized.map(|_| file.encode(&modes)).transpose()?;
    let observed = observation.map(|_| observe_compact_exanimation(&file.animation).to_text());
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
        return Err("ExAnimation input and size-mode table must differ");
    }
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|output| output == input || output == size_modes)
    {
        return Err("ExAnimation outputs must differ from every input");
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized ExAnimation and observation outputs must differ");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{CompactExAnimation, ExAnimationRecord};

    fn directory() -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("lm-cli-exanimation-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
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
    fn interpretation_normalization_and_observation_are_bound() {
        let directory = directory();
        let input = directory.join("input.lmexan");
        let modes_path = directory.join("modes.bin");
        let normalized = directory.join("normalized.lmexan");
        let observation = directory.join("animation.obs");
        let modes = [false; 256];
        let file = CompactExAnimationFile {
            source_slot: 3,
            animation: CompactExAnimation {
                setting: 0,
                header_value: 0,
                trigger_mask: 0,
                trigger_values: [0; 16],
                records: vec![
                    ExAnimationRecord::new(1, 0, 2, 0x123, false, &[7, 8], false).unwrap(),
                ],
            },
        };
        fs::write(&input, file.encode(&modes).unwrap()).unwrap();
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
            CompactExAnimationFile::decode(&fs::read(&normalized).unwrap(), 32, &modes).unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
        assert_eq!(observed.get("exanimation/record-count"), Some("1"));
        fs::remove_dir_all(directory).unwrap();
    }
}
