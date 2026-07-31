use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_app::{GraphicsOwnershipFile, PaletteOwnershipFile};
use lm_graphics::{GraphicsTileOwner, PaletteEntryOwner};
use lm_oracle::Observation;
use std::path::Path;

pub fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::GraphicsOwnershipFile {
            input,
            normalized_output,
            observation,
        } => execute_graphics(input, normalized_output.as_deref(), observation.as_deref())?,
        crate::command_types::Command::PaletteOwnershipFile {
            input,
            normalized_output,
            observation,
        } => execute_palette(input, normalized_output.as_deref(), observation.as_deref())?,
        _ => return Ok(false),
    }
    Ok(true)
}

pub fn execute_graphics(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation, "graphics ownership")?;
    let file =
        GraphicsOwnershipFile::decode(&read_bounded(input, GraphicsOwnershipFile::MAX_FILE_LEN)?)?;
    println!("tiles: {}", file.ownership.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation
        .map(|_| observe_graphics(&file).map(|value| value.to_text()))
        .transpose()?;
    publish(
        normalized,
        observation,
        encoded.as_deref(),
        observed.as_deref(),
    )
}

pub fn execute_palette(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation, "palette ownership")?;
    let file =
        PaletteOwnershipFile::decode(&read_bounded(input, PaletteOwnershipFile::MAX_FILE_LEN)?)?;
    println!("colors: {}", file.ownership.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation
        .map(|_| observe_palette(&file).map(|value| value.to_text()))
        .transpose()?;
    publish(
        normalized,
        observation,
        encoded.as_deref(),
        observed.as_deref(),
    )
}

fn observe_graphics(
    file: &GraphicsOwnershipFile,
) -> Result<Observation, lm_oracle::ObservationError> {
    let mut observed = Observation::new();
    observed.insert("ownership/domain", "graphics")?;
    observed.insert("ownership/count", file.ownership.len().to_string())?;
    for index in 0..file.ownership.len() {
        let base = format!("ownership/entries/{index:04x}");
        match file
            .ownership
            .owner(index)
            .expect("bounded ownership index")
        {
            GraphicsTileOwner::Editable => observed.insert(format!("{base}/owner"), "editable")?,
            GraphicsTileOwner::Fixed => observed.insert(format!("{base}/owner"), "fixed")?,
            GraphicsTileOwner::ExAnimation { record } => {
                observed.insert(format!("{base}/owner"), "exanimation")?;
                observed.insert(format!("{base}/record"), record.to_string())?;
            }
            GraphicsTileOwner::OriginalAnimation { slot } => {
                observed.insert(format!("{base}/owner"), "original-animation")?;
                observed.insert(format!("{base}/slot"), slot.to_string())?;
            }
            GraphicsTileOwner::LevelExAnimation { slot } => {
                observed.insert(format!("{base}/owner"), "level-exanimation")?;
                observed.insert(format!("{base}/slot"), slot.to_string())?;
            }
            GraphicsTileOwner::GlobalExAnimation { slot } => {
                observed.insert(format!("{base}/owner"), "global-exanimation")?;
                observed.insert(format!("{base}/slot"), slot.to_string())?;
            }
        }
    }
    Ok(observed)
}

fn observe_palette(
    file: &PaletteOwnershipFile,
) -> Result<Observation, lm_oracle::ObservationError> {
    let mut observed = Observation::new();
    observed.insert("ownership/domain", "palette")?;
    observed.insert("ownership/count", file.ownership.len().to_string())?;
    for index in 0..file.ownership.len() {
        let base = format!("ownership/entries/{index:04x}");
        match file
            .ownership
            .owner(index)
            .expect("bounded ownership index")
        {
            PaletteEntryOwner::Editable => observed.insert(format!("{base}/owner"), "editable")?,
            PaletteEntryOwner::Fixed => observed.insert(format!("{base}/owner"), "fixed")?,
            PaletteEntryOwner::ExAnimation { record } => {
                observed.insert(format!("{base}/owner"), "exanimation")?;
                observed.insert(format!("{base}/record"), record.to_string())?;
            }
        }
    }
    Ok(observed)
}

fn validate_paths(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
    domain: &'static str,
) -> Result<(), String> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err(format!("{domain} outputs must differ from input"));
    }
    if normalized.is_some() && normalized == observation {
        return Err(format!(
            "normalized {domain} and observation outputs must differ"
        ));
    }
    Ok(())
}

fn publish(
    normalized_path: Option<&Path>,
    observation_path: Option<&Path>,
    normalized: Option<&[u8]>,
    observation: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized_path, normalized) {
        outputs.push((path, bytes));
    }
    if let (Some(path), Some(text)) = (observation_path, observation) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_output_collisions_are_rejected_before_io() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute_graphics(input, Some(input), None).is_err());
        assert!(execute_palette(input, None, Some(input)).is_err());
        assert!(execute_graphics(input, Some(output), Some(output)).is_err());
    }
}
