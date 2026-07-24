use crate::{atomic_output::write_new_batch, command_types::Command, oracle_input::read_bounded};
use lm_level::{
    LAYER3_TILEMAP_WORKSPACE_LEN, Layer3TilemapGraphicsDescriptor, Layer3TilemapWorkspace,
};
use lm_oracle::{Observation, sha256_hex};
use std::path::Path;

const MAX_DECODED_GRAPHICS_LEN: usize = 0x1_0000;

pub fn execute_command(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    let Command::Layer3WorkspaceApply {
        packed_descriptor,
        workspace,
        decoded_graphics,
        output,
        observation,
    } = command
    else {
        return Ok(false);
    };
    execute(
        *packed_descriptor,
        workspace,
        decoded_graphics,
        output,
        observation.as_deref(),
    )?;
    Ok(true)
}

fn execute(
    packed_descriptor: u16,
    workspace_path: &Path,
    graphics_path: &Path,
    output: &Path,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if output == workspace_path || output == graphics_path {
        return Err("Layer 3 workspace output must differ from both inputs".into());
    }
    if observation
        .is_some_and(|path| path == workspace_path || path == graphics_path || path == output)
    {
        return Err("Layer 3 workspace observation must differ from inputs and output".into());
    }
    let input = read_bounded(workspace_path, LAYER3_TILEMAP_WORKSPACE_LEN + 1)?;
    let graphics = read_bounded(graphics_path, MAX_DECODED_GRAPHICS_LEN)?;
    let descriptor = Layer3TilemapGraphicsDescriptor::from_packed(packed_descriptor);
    let mut workspace = Layer3TilemapWorkspace::decode(&input)?;
    workspace.apply_decoded_file(descriptor, &graphics)?;
    let observed = observation.map(|_| observe(descriptor, &input, &graphics, &workspace));
    let mut outputs: Vec<(&Path, &[u8])> = vec![(output, workspace.encoded())];
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    Ok(())
}

fn observe(
    descriptor: Layer3TilemapGraphicsDescriptor,
    before: &[u8],
    graphics: &[u8],
    after: &Layer3TilemapWorkspace,
) -> String {
    let mut result = Observation::new();
    for (path, value) in [
        (
            "layer3-workspace/descriptor/packed",
            format!("{:04x}", descriptor.packed()),
        ),
        (
            "layer3-workspace/descriptor/file",
            format!("{:03x}", descriptor.file()),
        ),
        (
            "layer3-workspace/descriptor/length-selector",
            descriptor.length_selector().to_string(),
        ),
        (
            "layer3-workspace/descriptor/offset-selector",
            descriptor.offset_selector().to_string(),
        ),
        (
            "layer3-workspace/destination-byte-offset",
            descriptor.destination_byte_offset().to_string(),
        ),
        (
            "layer3-workspace/effective-byte-length",
            descriptor.effective_byte_length().to_string(),
        ),
        ("layer3-workspace/before-sha256", sha256_hex(before)),
        ("layer3-workspace/graphics-sha256", sha256_hex(graphics)),
        (
            "layer3-workspace/selected-sha256",
            sha256_hex(after.selected_range(descriptor)),
        ),
        ("layer3-workspace/after-sha256", sha256_hex(after.encoded())),
    ] {
        result
            .insert(path, value)
            .expect("Layer 3 workspace observation paths are unique");
    }
    result.to_text()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn command_applies_clipped_range_and_observes_it_atomically() {
        let directory =
            std::env::temp_dir().join(format!("lm-cli-layer3-workspace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("input.bin");
        let graphics = directory.join("graphics.bin");
        let output = directory.join("output.bin");
        let observation = directory.join("output.obs");
        fs::write(&input, vec![0xa5; LAYER3_TILEMAP_WORKSPACE_LEN]).unwrap();
        fs::write(&graphics, vec![0x5a; LAYER3_TILEMAP_WORKSPACE_LEN]).unwrap();
        execute(0xc028, &input, &graphics, &output, Some(&observation)).unwrap();
        let bytes = fs::read(output).unwrap();
        assert_eq!(&bytes[..0x1000], &[0xa5; 0x1000]);
        assert_eq!(&bytes[0x1000..], &[0x5a; 0x1000]);
        let observed = Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(
            observed.get("layer3-workspace/effective-byte-length"),
            Some("4096")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aliases_and_short_inputs_never_publish() {
        let path = Path::new("same");
        assert!(execute(0, path, Path::new("graphics"), path, None).is_err());
    }
}
