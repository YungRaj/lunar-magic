use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_overworld_paths;
use lm_overworld::{OverworldPathGraph, PathGraphError};
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
            return Err("overworld path outputs must differ from input".into());
        }
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized path and observation outputs must differ".into());
    }
    let graph =
        OverworldPathGraph::decode_file(&read_bounded(input, OverworldPathGraph::MAX_FILE_LEN)?)?;
    println!("nodes: {}", graph.nodes.len());
    println!("edges: {}", graph.edges.len());
    let reciprocal_error = match graph.validate_reciprocal() {
        Ok(()) => {
            println!("reciprocal: yes");
            None
        }
        Err(PathGraphError::MissingReciprocal { edge }) => {
            println!("reciprocal: no (edge {edge})");
            Some(PathGraphError::MissingReciprocal { edge })
        }
        Err(error) => return Err(error.into()),
    };
    let normalized = if normalized_output.is_some() {
        if let Some(error) = reciprocal_error {
            return Err(error.into());
        }
        Some(graph.encode_file()?)
    } else {
        None
    };
    let observed = observation.map(|_| observe_overworld_paths(&graph).to_text());
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
        let path = std::env::temp_dir().join(format!("lm-cli-path-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn normalization_cannot_overwrite_its_input() {
        let path = Path::new("same.lmowpath");
        assert!(execute(path, Some(path), None).is_err());
        assert!(execute(path, None, Some(path)).is_err());
        let output = Path::new("same-output");
        assert!(execute(path, Some(output), Some(output)).is_err());
    }

    #[test]
    fn normalization_and_observation_publish_together() {
        let directory = directory();
        let input = directory.join("input.lmowpath");
        let normalized = directory.join("normalized.lmowpath");
        let observation = directory.join("paths.obs");
        let graph = OverworldPathGraph::default();
        fs::write(&input, graph.encode_file().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            OverworldPathGraph::decode_file(&fs::read(&normalized).unwrap()).unwrap(),
            graph
        );
        let text = fs::read_to_string(&observation).unwrap();
        let observed = lm_oracle::Observation::from_text(&text).unwrap();
        assert_eq!(observed.get("overworld/paths/nodes/count"), Some("0"));
        assert_eq!(observed.get("overworld/paths/edges/count"), Some("0"));
        fs::remove_dir_all(directory).unwrap();
    }
}
