use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_overworld_metadata;
use lm_overworld::OverworldMetadata;
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
            return Err("overworld metadata outputs must differ from input".into());
        }
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized metadata and observation outputs must differ".into());
    }
    let metadata =
        OverworldMetadata::decode_file(&read_bounded(input, OverworldMetadata::MAX_FILE_LEN)?)?;
    println!("level-names: {}", metadata.level_names.len());
    println!("player-starts: {}", metadata.player_starts.len());
    println!("submap-settings: {}", metadata.submap_settings.len());
    let normalized = normalized_output
        .map(|_| metadata.encode_file())
        .transpose()?;
    let observed = observation.map(|_| observe_overworld_metadata(&metadata).to_text());
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

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("lm-cli-metadata-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn normalization_cannot_overwrite_its_input() {
        let path = Path::new("same.lmowmeta");
        assert!(execute(path, Some(path), None).is_err());
        assert!(execute(path, None, Some(path)).is_err());
        let output = Path::new("same-output");
        assert!(execute(path, Some(output), Some(output)).is_err());
    }

    #[test]
    fn normalization_and_observation_publish_together() {
        let directory = directory();
        let input = directory.join("input.lmowmeta");
        let normalized = directory.join("normalized.lmowmeta");
        let observation = directory.join("metadata.obs");
        let metadata = OverworldMetadata::default();
        fs::write(&input, metadata.encode_file().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            OverworldMetadata::decode_file(&fs::read(&normalized).unwrap()).unwrap(),
            metadata
        );
        let text = fs::read_to_string(&observation).unwrap();
        let observed = lm_oracle::Observation::from_text(&text).unwrap();
        assert_eq!(
            observed.get("overworld/metadata/level-names/count"),
            Some("0")
        );
        assert_eq!(
            observed.get("overworld/metadata/player-starts/count"),
            Some("0")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
