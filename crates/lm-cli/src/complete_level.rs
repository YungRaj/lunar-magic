use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_level::CompleteLevelFile;
use lm_oracle::observe_level;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    for destination in [normalized_output, observation].into_iter().flatten() {
        if destination == input {
            return Err("level bundle outputs must differ from input".into());
        }
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized bundle and observation outputs must differ".into());
    }
    let file = CompleteLevelFile::decode(&read_bounded(input, CompleteLevelFile::MAX_FILE_LEN)?)?;
    println!("level: {:03X}", file.0.number);
    println!("layer1-objects: {}", file.0.layer1.objects.records.len());
    println!("layer2-objects: {}", file.0.layer2.objects.records.len());
    match &file.0.layer3 {
        Some(layer3) => println!(
            "layer3: {} tilemap bytes, {} remap bytes",
            layer3.tilemap.len(),
            layer3.remap_commands.len()
        ),
        None => println!("layer3: none"),
    }
    println!("sprites: {}", file.0.sprites.records.len());
    println!("entrances: {}", file.0.entrances.len());
    println!("screen-exits: {}", file.0.screen_exits.len());
    println!("secondary-exits: {}", file.0.secondary_exits.len());
    println!("map16-overrides: {}", file.0.map16_overrides.len());

    let normalized = normalized_output.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_level(&file.0).to_text());
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

    #[test]
    fn outputs_cannot_alias_inputs_or_each_other() {
        let input = Path::new("level.lmlevel");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, None, Some(input)).is_err());
        let output = Path::new("output.lmlevel");
        assert!(execute(input, Some(output), Some(output)).is_err());
    }
}
