use crate::{atomic_output::write_new_batch, oracle_input::read_bounded, sprite_length_file};
use lm_level::NativeLevelFile;
use lm_oracle::observe_native_level;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    sprite_lengths: Option<&Path>,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("native level outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized native level and observation outputs must differ".into());
    }
    let lengths = sprite_length_file::read(sprite_lengths)?;
    let file = NativeLevelFile::decode(
        &read_bounded(input, NativeLevelFile::MAX_FILE_LEN)?,
        &lengths,
    )?;
    println!("source level: {:#05x}", file.source_level);
    println!("objects: {}", file.layer1.objects.records.len());
    println!("sprite tokens: {}", file.sprites.tokens.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_native_level(&file).to_text());
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
    use lm_level::{LevelObjectData, NativeSpriteStream, SpriteLengthTable};

    #[test]
    fn interpretation_bound_native_level_round_trips_and_observes() {
        assert!(execute(Path::new("same"), None, Some(Path::new("same")), None).is_err());
        let directory =
            std::env::temp_dir().join(format!("lm-cli-native-level-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("level.lmlvl");
        let normalized = directory.join("normalized.lmlvl");
        let observation = directory.join("level.obs");
        let file = NativeLevelFile {
            source_level: 0x105,
            layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(
                &[0x10, 0x20, 1, 2, 0xff],
                false,
                &SpriteLengthTable::standard(),
            )
            .unwrap(),
        };
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, None, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            NativeLevelFile::decode(
                &fs::read(normalized).unwrap(),
                &SpriteLengthTable::standard()
            )
            .unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(
            observed.get("native-level/sprites/tokens/0000/encoded"),
            Some("200102")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
