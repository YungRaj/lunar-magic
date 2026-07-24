use crate::{
    atomic_output::write_new_batch,
    command_types::{Command, NativeMap16SidecarKind},
    oracle_input::read_bounded,
};
use lm_level::{M16Sidecar, S16Sidecar};
use lm_oracle::{observe_m16_sidecar, observe_s16_sidecar};
use std::path::Path;

pub fn execute_command(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    let Command::NativeMap16Sidecar {
        kind,
        input,
        normalized_output,
        observation,
    } = command
    else {
        return Ok(false);
    };
    execute(
        *kind,
        input,
        normalized_output.as_deref(),
        observation.as_deref(),
    )?;
    Ok(true)
}

pub fn execute(
    kind: NativeMap16SidecarKind,
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation)?;
    let maximum = match kind {
        NativeMap16SidecarKind::M16 => M16Sidecar::ENCODED_LEN,
        NativeMap16SidecarKind::S16 => S16Sidecar::CAPACITY,
    };
    let bytes = read_bounded(input, maximum)?;
    let (encoded, observed, entries, canonical_len) = match kind {
        NativeMap16SidecarKind::M16 => {
            let sidecar = M16Sidecar::decode(&bytes)?;
            (
                normalized.map(|_| sidecar.encode()),
                observation.map(|_| observe_m16_sidecar(&sidecar).to_text()),
                M16Sidecar::ENTRY_COUNT,
                M16Sidecar::ENCODED_LEN,
            )
        }
        NativeMap16SidecarKind::S16 => {
            let sidecar = S16Sidecar::decode(&bytes)?;
            (
                normalized.map(|_| sidecar.encode_canonical()),
                observation.map(|_| observe_s16_sidecar(&sidecar).to_text()),
                S16Sidecar::ENTRY_COUNT,
                sidecar.canonical_len(),
            )
        }
    };
    println!("entries: {entries}");
    println!("loaded-bytes: {}", bytes.len());
    println!("canonical-bytes: {canonical_len}");
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
    if normalized == Some(input)
        || observation == Some(input)
        || normalized.is_some() && normalized == observation
    {
        Err("native Map16 sidecar inputs and outputs must use distinct paths")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_rejected_before_file_access() {
        let input = Path::new("input.s16");
        assert!(execute(NativeMap16SidecarKind::S16, input, Some(input), None).is_err());
        assert!(execute(NativeMap16SidecarKind::S16, input, None, Some(input)).is_err());
        assert!(
            execute(
                NativeMap16SidecarKind::S16,
                input,
                Some(Path::new("same")),
                Some(Path::new("same"))
            )
            .is_err()
        );
    }
}
