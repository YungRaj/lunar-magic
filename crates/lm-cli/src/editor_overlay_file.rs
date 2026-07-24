use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_render::{EditorOverlay, EditorOverlayFile, observe_editor_overlays};
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
        return Err("editor-overlay outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized editor overlays and observation outputs must differ".into());
    }
    let file = EditorOverlayFile::decode(&read_bounded(input, EditorOverlayFile::MAX_FILE_LEN)?)?;
    let grids = file
        .overlays
        .iter()
        .filter(|overlay| matches!(overlay, EditorOverlay::Grid(_)))
        .count();
    println!("overlays: {}", file.overlays.len());
    println!("grids: {grids}");
    println!("selections: {}", file.overlays.len() - grids);

    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_editor_overlays(&file).to_text());
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized, encoded.as_deref()) {
        outputs.push((path, bytes));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_deref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_render::{GridOverlay, Rgba};
    use std::fs;

    #[test]
    fn normalization_and_observation_publish_as_one_distinct_batch() {
        assert!(execute(Path::new("same"), Some(Path::new("same")), None).is_err());
        assert!(
            execute(
                Path::new("input"),
                Some(Path::new("output")),
                Some(Path::new("output"))
            )
            .is_err()
        );
        let directory =
            std::env::temp_dir().join(format!("lm-cli-editor-overlay-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("overlays.lmovly");
        let normalized = directory.join("normalized.lmovly");
        let observation = directory.join("overlays.obs");
        let file = EditorOverlayFile {
            overlays: vec![EditorOverlay::Grid(GridOverlay {
                origin_x: -1,
                origin_y: 2,
                cell_width: 8,
                cell_height: 16,
                color: Rgba {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 4,
                },
            })],
        };
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            EditorOverlayFile::decode(&fs::read(&normalized).unwrap()).unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
        assert_eq!(observed.get("editor-overlays/0000/origin-x"), Some("-1"));
        fs::remove_dir_all(directory).unwrap();
    }
}
