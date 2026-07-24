use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_level::{CustomSpriteLibrary, MAX_CUSTOM_SPRITE_SIDECAR_LEN, SpriteLengthTable};
use lm_oracle::observe_custom_sprite_library;
use std::path::Path;

pub fn execute(
    data: &Path,
    descriptions: &Path,
    sprite_lengths: &Path,
    normalized_outputs: Option<(&Path, &Path)>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(
        data,
        descriptions,
        sprite_lengths,
        normalized_outputs,
        observation,
    )?;
    let lengths = SpriteLengthTable::decode(&read_bounded(
        sprite_lengths,
        SpriteLengthTable::ENCODED_LEN,
    )?)
    .map_err(|len| format!("sprite length table must contain exactly 1024 bytes, got {len}"))?;
    let library = CustomSpriteLibrary::decode(
        &read_bounded(data, MAX_CUSTOM_SPRITE_SIDECAR_LEN)?,
        &read_bounded(descriptions, MAX_CUSTOM_SPRITE_SIDECAR_LEN)?,
        &lengths,
    )?;
    println!("custom-sprite-placements: {}", library.entries().len());
    println!("header: {:02x}", library.header());

    let normalized = normalized_outputs
        .map(|_| library.encode_checked(&lengths))
        .transpose()?;
    let observed = observation.map(|_| observe_custom_sprite_library(&library).to_text());
    let mut outputs = Vec::new();
    if let (Some((output_data, output_descriptions)), Some((encoded_data, encoded_descriptions))) =
        (normalized_outputs, normalized.as_ref())
    {
        outputs.push((output_data, encoded_data.as_slice()));
        outputs.push((output_descriptions, encoded_descriptions.as_slice()));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

fn validate_paths(
    data: &Path,
    descriptions: &Path,
    sprite_lengths: &Path,
    normalized_outputs: Option<(&Path, &Path)>,
    observation: Option<&Path>,
) -> Result<(), &'static str> {
    let mut paths = vec![data, descriptions, sprite_lengths];
    if let Some((first, second)) = normalized_outputs {
        paths.extend([first, second]);
    }
    if let Some(observation) = observation {
        paths.push(observation);
    }
    for (index, path) in paths.iter().enumerate() {
        if paths[..index].contains(path) {
            return Err("custom-sprite inputs and outputs must all use distinct paths");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_and_output_path_must_differ() {
        let data = Path::new("sprites.mw2");
        let text = Path::new("sprites.mwt");
        let lengths = Path::new("lengths.bin");
        assert!(validate_paths(data, data, lengths, None, None).is_err());
        assert!(validate_paths(data, text, data, None, None).is_err());
        assert!(
            validate_paths(
                data,
                text,
                lengths,
                Some((Path::new("out"), Path::new("out"))),
                None
            )
            .is_err()
        );
        assert!(validate_paths(data, text, lengths, None, Some(text)).is_err());
    }
}
