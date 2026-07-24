use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_overworld_appearances;
use lm_overworld::SpriteAppearanceFile;
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
        return Err("overworld appearance outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized overworld appearances and observation outputs must differ".into());
    }
    let file =
        SpriteAppearanceFile::decode(&read_bounded(input, SpriteAppearanceFile::MAX_FILE_LEN)?)?;
    let parts: usize = file.definitions.iter().map(|value| value.parts.len()).sum();
    println!("definitions: {}", file.definitions.len());
    println!("parts: {parts}");
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_overworld_appearances(&file).to_text());
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
    use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};

    #[test]
    fn normalizes_and_observes_as_one_distinct_batch() {
        assert!(execute(Path::new("same"), Some(Path::new("same")), None).is_err());
        let directory = std::env::temp_dir().join(format!(
            "lm-cli-overworld-appearance-file-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("sprites.lmowapp");
        let normalized = directory.join("normalized.lmowapp");
        let observation = directory.join("sprites.obs");
        let file = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 0x81,
                parts: vec![SpriteAppearancePart {
                    tile_index: 7,
                    palette_index: 2,
                    x_offset: -3,
                    y_offset: 4,
                    x_flip: false,
                    y_flip: true,
                }],
            }],
        };
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            SpriteAppearanceFile::decode(&fs::read(normalized).unwrap()).unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(
            observed.get("overworld-appearances/definitions/0081/parts/0000/y"),
            Some("4")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
