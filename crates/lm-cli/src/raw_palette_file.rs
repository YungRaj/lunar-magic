use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_graphics::{PaletteMaskFile, RawSnesPaletteFile};
use lm_oracle::{observe_palette_mask, observe_raw_palette};
use std::path::Path;

pub fn execute_palette(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation)?;
    let file = RawSnesPaletteFile::decode(&read_bounded(input, RawSnesPaletteFile::FILE_LEN)?)?;
    println!("colors: {}", file.palette.colors.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_raw_palette(&file).to_text());
    publish(
        normalized,
        encoded.as_deref(),
        observation,
        observed.as_deref(),
    )
}

pub fn execute_mask(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation)?;
    let file = PaletteMaskFile::decode(&read_bounded(input, PaletteMaskFile::FILE_LEN)?)?;
    println!("entries: {}", file.entries().len());
    println!(
        "selected: {}",
        file.entries().iter().filter(|value| **value != 0).count()
    );
    let encoded = normalized.map(|_| file.encode());
    let observed = observation.map(|_| observe_palette_mask(&file).to_text());
    publish(
        normalized,
        encoded.as_deref(),
        observation,
        observed.as_deref(),
    )
}

fn publish(
    normalized: Option<&Path>,
    encoded: Option<&[u8]>,
    observation: Option<&Path>,
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
        return Err("raw palette outputs must differ from input");
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized raw palette and observation outputs must differ");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_workflows_reject_output_aliases_before_reading() {
        let input = Path::new("input");
        let output = Path::new("output");
        for execute in [execute_palette, execute_mask] {
            assert!(execute(input, Some(input), None).is_err());
            assert!(execute(input, None, Some(input)).is_err());
            assert!(execute(input, Some(output), Some(output)).is_err());
        }
    }
}
