use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_expanded_layer_tilemap;
use lm_overworld::ExpandedLayerTilemap;
use std::path::Path;

pub(crate) fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("layer-tilemap outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized layer tilemap and observation outputs must differ".into());
    }
    let tilemap = ExpandedLayerTilemap::decode_native_file(&read_bounded(
        input,
        ExpandedLayerTilemap::FILE_LEN,
    )?)?;
    let encoded = normalized.map(|_| tilemap.encode_native_file());
    let observed =
        observation.map(|_| observe_expanded_layer_tilemap(&tilemap).map(|value| value.to_text()));
    let observed = observed.transpose()?;
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized, encoded.as_ref()) {
        outputs.push((path, bytes.as_slice()));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    println!(
        "layer-tilemap-primary-bytes: {}",
        tilemap.primary_bytes().len()
    );
    println!(
        "layer-tilemap-secondary-blank: {}",
        tilemap.secondary_is_blank()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn normalizes_and_observes_both_planes_atomically() {
        let directory =
            std::env::temp_dir().join(format!("lm-cli-layer-tilemap-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("title.lmtile");
        let normalized = directory.join("normalized.lmtile");
        let observation = directory.join("title.obs");
        let mut tilemap = ExpandedLayerTilemap::default();
        tilemap.primary_bytes_mut()[3] = 0x5a;
        fs::write(&input, tilemap.encode_native_file()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            ExpandedLayerTilemap::decode_native_file(&fs::read(normalized).unwrap()).unwrap(),
            tilemap
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(
            observed.get("scene/layer-tilemap/secondary-blank"),
            Some("true")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aliases_and_malformed_inputs_publish_nothing() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());

        let directory =
            std::env::temp_dir().join(format!("lm-cli-layer-tilemap-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let malformed = directory.join("bad");
        let normalized = directory.join("normalized");
        let observation = directory.join("observation");
        fs::write(&malformed, b"bad").unwrap();
        assert!(execute(&malformed, Some(&normalized), Some(&observation)).is_err());
        assert!(!normalized.exists());
        assert!(!observation.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
