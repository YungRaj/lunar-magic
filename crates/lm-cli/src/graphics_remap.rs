use crate::{atomic_output::write_new_batch, command_types::Command, oracle_input::read_bounded};
use lm_graphics::{
    GRAPHICS_REMAP_MAX_PREFIX_LEN, GRAPHICS_REMAP_WORDS, GraphicsRemapCommandStream,
};
#[cfg(test)]
use lm_oracle::Observation;
use lm_oracle::{observe_graphics_remap, sha256_hex};
use std::path::Path;

const SCRATCH_BYTES: usize = GRAPHICS_REMAP_WORDS * 2;

pub fn execute_command(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::GraphicsRemapFile {
            input,
            normalized_output,
            observation,
        } => inspect(input, normalized_output.as_deref(), observation.as_deref())?,
        Command::GraphicsRemapApply {
            stream,
            scratch,
            output,
            observation,
        } => apply(stream, scratch, output, observation.as_deref())?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn inspect(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_outputs(&[input], normalized, observation)?;
    let bytes = read_bounded(input, GRAPHICS_REMAP_MAX_PREFIX_LEN)?;
    let decoded = GraphicsRemapCommandStream::decode_prefix(&bytes)?;
    println!("commands: {}", decoded.stream.commands.len());
    println!("consumed: {}", decoded.consumed);
    let normalized_bytes = normalized.map(|_| decoded.stream.encode()).transpose()?;
    let observation_text = observation.map(|_| observe_graphics_remap(&decoded).to_text());
    publish_optional(
        normalized,
        normalized_bytes.as_deref(),
        observation,
        observation_text.as_deref(),
    )?;
    Ok(())
}

fn apply(
    stream_path: &Path,
    scratch_path: &Path,
    output: &Path,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_outputs(&[stream_path, scratch_path], Some(output), observation)?;
    let stream_bytes = read_bounded(stream_path, GRAPHICS_REMAP_MAX_PREFIX_LEN)?;
    let decoded = GraphicsRemapCommandStream::decode_prefix(&stream_bytes)?;
    let scratch_bytes = read_bounded(scratch_path, SCRATCH_BYTES + 1)?;
    if scratch_bytes.len() != SCRATCH_BYTES {
        return Err(format!(
            "graphics remap scratch requires {SCRATCH_BYTES} bytes, got {}",
            scratch_bytes.len()
        )
        .into());
    }
    let mut scratch: Vec<u16> = scratch_bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    decoded.stream.apply(&mut scratch)?;
    let output_bytes: Vec<u8> = scratch.iter().flat_map(|word| word.to_le_bytes()).collect();
    let observation_text = observation.map(|_| {
        let mut observed = observe_graphics_remap(&decoded);
        observed
            .insert(
                "graphics-remap/scratch-before-sha256",
                sha256_hex(&scratch_bytes),
            )
            .expect("scratch observation path is unique");
        observed
            .insert(
                "graphics-remap/scratch-after-sha256",
                sha256_hex(&output_bytes),
            )
            .expect("scratch observation path is unique");
        observed.to_text()
    });
    publish_optional(
        Some(output),
        Some(&output_bytes),
        observation,
        observation_text.as_deref(),
    )?;
    Ok(())
}

fn validate_outputs(
    inputs: &[&Path],
    output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if output.is_some_and(|path| inputs.contains(&path))
        || observation.is_some_and(|path| inputs.contains(&path) || Some(path) == output)
    {
        return Err("graphics remap inputs and outputs must use distinct paths".into());
    }
    Ok(())
}

fn publish_optional(
    output: Option<&Path>,
    bytes: Option<&[u8]>,
    observation: Option<&Path>,
    text: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (output, bytes) {
        outputs.push((path, bytes));
    }
    if let (Some(path), Some(text)) = (observation, text) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{
        GraphicsRemapCommand, GraphicsRemapEnd, GraphicsRemapPayload, GraphicsRemapStride,
    };
    use std::fs;

    fn stream() -> Vec<u8> {
        GraphicsRemapCommandStream {
            commands: vec![GraphicsRemapCommand {
                destination_word: 1,
                stride: GraphicsRemapStride::Linear,
                payload: GraphicsRemapPayload::Literal(vec![0x34, 0x12]),
            }],
            end: GraphicsRemapEnd::Terminator([0x80, 0, 0, 0]),
        }
        .encode()
        .unwrap()
    }

    #[test]
    fn inspection_and_application_publish_exact_outputs() {
        let directory =
            std::env::temp_dir().join(format!("lm-cli-graphics-remap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let stream_path = directory.join("stream.bin");
        let normalized = directory.join("normalized.bin");
        let stream_observation = directory.join("stream.obs");
        let scratch = directory.join("scratch.bin");
        let output = directory.join("output.bin");
        let apply_observation = directory.join("apply.obs");
        fs::write(&stream_path, stream()).unwrap();
        fs::write(&scratch, vec![0x5a; SCRATCH_BYTES]).unwrap();
        inspect(&stream_path, Some(&normalized), Some(&stream_observation)).unwrap();
        assert_eq!(fs::read(normalized).unwrap(), stream());
        apply(&stream_path, &scratch, &output, Some(&apply_observation)).unwrap();
        assert_eq!(&fs::read(output).unwrap()[2..4], &[0x34, 0x12]);
        let observed =
            Observation::from_text(&fs::read_to_string(apply_observation).unwrap()).unwrap();
        assert_eq!(
            observed.get("graphics-remap/commands/0000/destination-word"),
            Some("1")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn alias_validation_happens_before_input_reads() {
        let same = Path::new("same");
        assert!(inspect(same, Some(same), None).is_err());
        assert!(apply(same, Path::new("scratch"), same, None).is_err());
    }
}
