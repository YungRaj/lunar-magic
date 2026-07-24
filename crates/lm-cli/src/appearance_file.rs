use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_level::EntityAppearanceFile;
use lm_oracle::observe_entity_appearances;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation)?;
    let file =
        EntityAppearanceFile::decode(&read_bounded(input, EntityAppearanceFile::MAX_FILE_LEN)?)?;
    println!("appearances: {}", file.appearances.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_entity_appearances(&file).to_text());
    publish(
        normalized,
        observation,
        encoded.as_deref(),
        observed.as_deref(),
    )
}

fn validate_paths(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("entity appearance outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized entity appearances and observation outputs must differ".into());
    }
    Ok(())
}

fn publish(
    normalized: Option<&Path>,
    observation: Option<&Path>,
    encoded: Option<&[u8]>,
    observed: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized, encoded) {
        outputs.push((path, bytes));
    }
    if let (Some(path), Some(text)) = (observation, observed) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{AppearanceSource, EntityAppearanceRecord};

    #[test]
    fn normalizes_and_observes_as_one_distinct_batch() {
        assert!(execute(Path::new("same"), Some(Path::new("same")), None).is_err());
        assert!(
            execute(
                Path::new("input"),
                Some(Path::new("same")),
                Some(Path::new("same"))
            )
            .is_err()
        );
        let directory =
            std::env::temp_dir().join(format!("lm-cli-appearance-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("entity.lmentapp");
        let normalized = directory.join("normalized.lmentapp");
        let observation = directory.join("entity.obs");
        let file = EntityAppearanceFile {
            appearances: vec![EntityAppearanceRecord {
                source: AppearanceSource::Sprite(3),
                tile_index: 4,
                palette_index: 2,
                x: -5,
                y: 6,
                x_flip: true,
                y_flip: false,
            }],
        };
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            EntityAppearanceFile::decode(&fs::read(normalized).unwrap()).unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(
            observed.get("entity-appearances/records/000000/source-kind"),
            Some("sprite")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
