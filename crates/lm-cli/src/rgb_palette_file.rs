use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_graphics::RgbPaletteFile;
use lm_oracle::observe_rgb_palette;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation)?;
    let file = RgbPaletteFile::decode(&read_bounded(input, RgbPaletteFile::FILE_LEN)?)?;
    println!("colors: {}", file.colors.len());
    println!("expansion: {:?}", file.detected_expansion);
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_rgb_palette(&file).to_text());
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
        return Err("RGB palette outputs must differ from input");
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized RGB palette and observation outputs must differ");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_paths_must_be_distinct() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, None, Some(input)).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());
    }
}
