use crate::{
    atomic_output::{write_new, write_new_batch},
    oracle_input::read_bounded,
};
use lm_level::{ExpandedLevelSettingsRecord, Layer3TilemapGraphicsDescriptor};
use lm_oracle::observe_expanded_settings;
use std::path::Path;

pub fn execute_command(
    command: &crate::command_types::Command,
) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        crate::command_types::Command::ExpandedSettingsFile {
            input,
            normalized_output,
            observation,
        } => execute(input, normalized_output.as_deref(), observation.as_deref())?,
        crate::command_types::Command::ExpandedSettingsLayer3 {
            input,
            enabled,
            file,
            length_selector,
            offset_selector,
            output,
        } => edit_layer3(
            input,
            *enabled,
            *file,
            *length_selector,
            *offset_selector,
            output,
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn edit_layer3(
    input: &Path,
    enabled: bool,
    file: u16,
    length_selector: u8,
    offset_selector: u8,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("expanded-settings Layer 3 output must differ from input".into());
    }
    let mut record = ExpandedLevelSettingsRecord::decode(&read_bounded(
        input,
        ExpandedLevelSettingsRecord::ENCODED_LEN,
    )?)?;
    let descriptor = Layer3TilemapGraphicsDescriptor::new(file, length_selector, offset_selector)?;
    record.set_layer3_tilemap_enabled(enabled)?;
    record.set_layer3_tilemap_graphics_descriptor(descriptor)?;
    write_new(output, record.encoded())?;
    println!(
        "layer3-tilemap: {} file={:03x} offset={:04x} length={:04x}",
        if enabled { "enabled" } else { "disabled" },
        descriptor.file(),
        descriptor.destination_byte_offset(),
        descriptor.effective_byte_length()
    );
    Ok(())
}

pub fn execute(
    input: &Path,
    normalized_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    for output in [normalized_output, observation].into_iter().flatten() {
        if output == input {
            return Err("expanded-settings outputs must differ from input".into());
        }
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("normalized expanded settings and observation outputs must differ".into());
    }
    let record = ExpandedLevelSettingsRecord::decode(&read_bounded(
        input,
        ExpandedLevelSettingsRecord::ENCODED_LEN,
    )?)?;
    println!("words: {}", ExpandedLevelSettingsRecord::WORD_COUNT);
    let observed = observation.map(|_| observe_expanded_settings(&record).to_text());
    let mut outputs: Vec<(&Path, &[u8])> = Vec::new();
    if let Some(path) = normalized_output {
        outputs.push((path, record.encoded()));
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
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    #[test]
    fn normalization_and_observation_publish_as_one_exact_batch() {
        let dir = std::env::temp_dir().join(format!(
            "lm-expanded-settings-file-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("record.bin");
        let normalized = dir.join("normalized.bin");
        let observation = dir.join("record.obs");
        let bytes = std::array::from_fn::<_, 32, _>(|i| u8::try_from(i).unwrap());
        fs::write(&input, bytes).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(fs::read(&normalized).unwrap(), bytes);
        let observed = lm_oracle::Observation::from_text(
            std::str::from_utf8(&fs::read(&observation).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(observed.get("expanded-settings/words/0f"), Some("7966"));
        assert!(execute(&input, Some(&normalized), None).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn semantic_layer3_edit_is_create_new_and_preserves_unknown_bits() {
        let dir = std::env::temp_dir().join(format!(
            "lm-expanded-settings-layer3-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("record.bin");
        let output = dir.join("edited.bin");
        let mut bytes = [0; 32];
        bytes[..2].copy_from_slice(&0x8123_u16.to_le_bytes());
        bytes[4..].fill(0x5a);
        fs::write(&input, bytes).unwrap();
        edit_layer3(&input, true, 0xabc, 2, 3, &output).unwrap();
        let result = ExpandedLevelSettingsRecord::decode(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(result.word(0).unwrap(), 0xa123);
        assert_eq!(result.word(1).unwrap(), 0xeabc);
        assert_eq!(&result.encoded()[4..], &[0x5a; 28]);
        assert!(edit_layer3(&input, true, 0xabc, 2, 3, &output).is_err());
        assert!(edit_layer3(&input, true, 0xabc, 2, 3, &input).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
