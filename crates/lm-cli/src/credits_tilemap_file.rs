use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_credits_tilemap;
use lm_overworld::CreditsTilemap;
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
        return Err("credits-tilemap outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized credits tilemap and observation outputs must differ".into());
    }
    let tilemap =
        CreditsTilemap::decode_native_file(&read_bounded(input, CreditsTilemap::FILE_LEN)?)?;
    let encoded = normalized.map(|_| tilemap.encode_native_file());
    let observed = observation
        .map(|_| observe_credits_tilemap(&tilemap).map(|value| value.to_text()))
        .transpose()?;
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized, encoded.as_ref()) {
        outputs.push((path, bytes.as_slice()));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    println!("credits-tilemap-words: {}", tilemap.words().len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_profile::SMW_US_V1_CREDITS_BLANK_WORD;
    use std::fs;

    #[test]
    fn normalizes_and_observes_every_row_atomically() {
        let directory =
            std::env::temp_dir().join(format!("lm-cli-credits-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("credits.lmcred");
        let normalized = directory.join("normalized.lmcred");
        let observation = directory.join("credits.obs");
        let mut tilemap = CreditsTilemap::blank(SMW_US_V1_CREDITS_BLANK_WORD);
        tilemap.words_mut()[CreditsTilemap::COLUMNS + 3] = 0x1234;
        fs::write(&input, tilemap.encode_native_file()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            CreditsTilemap::decode_native_file(&fs::read(normalized).unwrap()).unwrap(),
            tilemap
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert!(observed.get("credits/tilemap/row/1/sha256").is_some());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_inputs_and_aliases_publish_nothing() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());

        let directory =
            std::env::temp_dir().join(format!("lm-cli-credits-bad-{}", std::process::id()));
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
