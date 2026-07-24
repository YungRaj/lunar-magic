use crate::{atomic_output::write_new_batch, command_types::Command, oracle_input::read_bounded};
use lm_level::{DscSidecar, MAX_DSC_SOURCE_LEN};
use lm_oracle::observe_dsc_sidecar;
use std::path::Path;

pub fn execute_command(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    let Command::DscSidecar {
        input,
        lossless_output,
        observation,
    } = command
    else {
        return Ok(false);
    };
    execute(input, lossless_output.as_deref(), observation.as_deref())?;
    Ok(true)
}

fn execute(
    input: &Path,
    lossless_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if lossless_output == Some(input)
        || observation == Some(input)
        || lossless_output.is_some() && lossless_output == observation
    {
        return Err("DSC input and outputs must use distinct paths".into());
    }
    let bytes = read_bounded(input, MAX_DSC_SOURCE_LEN)?;
    let sidecar = DscSidecar::decode(&bytes)?;
    let lossless = lossless_output.map(|_| sidecar.encode_lossless());
    let observed = observation.map(|_| observe_dsc_sidecar(&sidecar).to_text());
    println!("entries: {}", sidecar.entries().len());
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (lossless_output, lossless.as_ref()) {
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
    fn rejects_aliases_before_reading() {
        let input = Path::new("same.dsc");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, None, Some(input)).is_err());
    }
}
