use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_graphics::GraphicsInterchangeFile;
use lm_oracle::observe_graphics;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation)?;
    let file = GraphicsInterchangeFile::decode(&read_bounded(
        input,
        GraphicsInterchangeFile::MAX_FILE_LEN,
    )?)?;
    println!("source-slot: {}", file.source_slot);
    println!("tiles: {}", file.graphics.tiles.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_graphics(&file.graphics).to_text());
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
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), &'static str> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("graphics outputs must differ from input");
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized graphics and observation outputs must differ");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{GraphicsFile4bpp, IndexedTile};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-cli-graphics-file-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn outputs_must_be_distinct() {
        let input = Path::new("in");
        let output = Path::new("out");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, None, Some(input)).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());
    }

    #[test]
    fn normalization_and_observation_publish_together() {
        let directory = directory();
        let input = directory.join("input.lmgfx");
        let normalized = directory.join("normalized.lmgfx");
        let observation = directory.join("graphics.obs");
        let file = GraphicsInterchangeFile {
            source_slot: 7,
            graphics: GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([3; 64])],
            },
        };
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            GraphicsInterchangeFile::decode(&fs::read(&normalized).unwrap()).unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
        assert_eq!(observed.get("graphics/tile-count"), Some("1"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn oversized_graphics_file_is_rejected_without_outputs() {
        let directory = directory();
        let input = directory.join("oversized.lmgfx");
        let observation = directory.join("graphics.obs");
        fs::File::create(&input)
            .unwrap()
            .set_len(u64::try_from(GraphicsInterchangeFile::MAX_FILE_LEN + 1).unwrap())
            .unwrap();
        assert!(execute(&input, None, Some(&observation)).is_err());
        assert!(!observation.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
