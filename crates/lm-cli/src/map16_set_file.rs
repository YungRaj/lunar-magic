use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_level::{Map16Page, Map16SetFile};
use lm_oracle::observe_map16_set;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    for destination in [normalized_output, observation].into_iter().flatten() {
        if destination == input {
            return Err("Map16 set outputs must differ from input".into());
        }
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized Map16 set and observation outputs must differ".into());
    }
    let file = Map16SetFile::decode(&read_bounded(input, Map16SetFile::MAX_FILE_LEN)?)?;
    let tile_count = file
        .set
        .pages
        .len()
        .checked_mul(Map16Page::TILE_COUNT)
        .ok_or("Map16 tile count overflow")?;
    let resolution_limit = tile_count
        .checked_add(1)
        .ok_or("Map16 resolution limit overflow")?;
    let acts_like = file.set.validate_acts_like(resolution_limit);
    println!("pages: {}", file.set.pages.len());
    println!("tiles: {tile_count}");
    match &acts_like {
        Ok(()) => println!("acts-like-graph: valid"),
        Err(error) => println!("acts-like-graph: invalid ({error})"),
    }
    if normalized_output.is_some() {
        acts_like?;
    }
    let normalized = normalized_output.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_map16_set(&file.set).to_text());
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
        let input = Path::new("map16.lm16set");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, None, Some(input)).is_err());
        let output = Path::new("output.lm16set");
        assert!(execute(input, Some(output), Some(output)).is_err());
    }
}
