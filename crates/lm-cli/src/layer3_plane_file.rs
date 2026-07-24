use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_render::{MaterializedLayer3Plane, observe_materialized_layer3_plane};
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("Layer 3 plane outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized Layer 3 plane and observation outputs must differ".into());
    }
    let plane = MaterializedLayer3Plane::decode(&read_bounded(
        input,
        MaterializedLayer3Plane::MAX_FILE_LEN,
    )?)?;
    println!("placement: {:?}", plane.placement);
    println!("instances: {}", plane.instances.len());
    let encoded = normalized.map(|_| plane.encode()).transpose()?;
    let observed = observation.map(|_| observe_materialized_layer3_plane(&plane).to_text());
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

#[cfg(test)]
mod tests {
    use super::*;
    use lm_render::{Layer3Placement, TileInstance};

    #[test]
    fn provider_plane_round_trips_and_observes_atomically() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());
        let directory =
            std::env::temp_dir().join(format!("lm-cli-layer3-plane-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("plane.lml3frame");
        let normalized = directory.join("normalized.lml3frame");
        let observation = directory.join("plane.obs");
        let plane = MaterializedLayer3Plane {
            source_digest: [7; 32],
            placement: Layer3Placement::AboveLayer1,
            instances: vec![TileInstance {
                tile_index: 4,
                palette_index: 3,
                x: -2,
                y: 5,
                x_flip: false,
                y_flip: true,
            }],
        };
        fs::write(&input, plane.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            MaterializedLayer3Plane::decode(&fs::read(normalized).unwrap()).unwrap(),
            plane
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(observed.get("layer3-plane/instances/0000/y"), Some("5"));
        fs::remove_dir_all(directory).unwrap();
    }
}
