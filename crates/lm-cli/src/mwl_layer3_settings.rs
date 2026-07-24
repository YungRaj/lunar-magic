use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_level::{Layer3TilemapGraphicsDescriptor, MwlFile};
use std::path::Path;

pub fn execute(
    input: &Path,
    enabled: bool,
    file: u16,
    length_selector: u8,
    offset_selector: u8,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("MWL Layer 3 settings output must differ from input".into());
    }
    let mut mwl = MwlFile::decode(&read_bounded(input, MwlFile::MAX_FILE_BYTES)?)?;
    let mut settings = mwl.expanded_settings_section()?;
    let descriptor = Layer3TilemapGraphicsDescriptor::new(file, length_selector, offset_selector)?;
    settings.set_layer3_tilemap_enabled(enabled)?;
    settings.set_layer3_tilemap_graphics_descriptor(descriptor)?;
    mwl.set_expanded_settings_section(&settings);
    let encoded = mwl.encode()?;
    let reopened = MwlFile::decode(&encoded)?;
    if reopened.expanded_settings_section()? != settings {
        return Err("edited MWL Layer 3 settings failed semantic reopen".into());
    }
    write_new(output, encoded)?;
    println!(
        "edited-mwl-layer3: {} file={:03x} offset={:04x} length={:04x}",
        if enabled { "enabled" } else { "disabled" },
        descriptor.file(),
        descriptor.destination_byte_offset(),
        descriptor.effective_byte_length()
    );
    Ok(())
}

pub fn observe(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("MWL and Layer 3 observation paths must differ".into());
    }
    let mwl = MwlFile::decode(&read_bounded(input, MwlFile::MAX_FILE_BYTES)?)?;
    let settings = mwl.expanded_settings_section()?;
    write_new(
        output,
        lm_oracle::observe_expanded_settings(&settings).to_text(),
    )?;
    println!("observed-mwl-layer3-settings: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{ExpandedLevelSettingsRecord, MwlSectionKind};
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn edits_only_recovered_words_and_publishes_create_new() {
        let directory = std::env::temp_dir().join(format!(
            "lm-mwl-layer3-settings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let input = directory.join("input.mwl");
        let output = directory.join("output.mwl");
        let mut settings = [0x5a; 32];
        settings[..2].copy_from_slice(&0x8123_u16.to_le_bytes());
        let mut source = MwlFile::default();
        source.set_expanded_settings_section(
            &ExpandedLevelSettingsRecord::decode(&settings).unwrap(),
        );
        source.set_section(MwlSectionKind::Layer1, vec![1, 2, 3]);
        fs::write(&input, source.encode().unwrap()).unwrap();
        execute(&input, true, 0xabc, 2, 3, &output).unwrap();
        let result = MwlFile::decode(&fs::read(&output).unwrap()).unwrap();
        let settings = result.expanded_settings_section().unwrap();
        assert_eq!(settings.word(0).unwrap(), 0xa123);
        assert_eq!(settings.word(1).unwrap(), 0xeabc);
        assert_eq!(&settings.encoded()[4..], &[0x5a; 28]);
        assert_eq!(result.section(MwlSectionKind::Layer1), [1, 2, 3]);
        assert!(execute(&input, true, 0xabc, 2, 3, &output).is_err());
        assert!(execute(&input, true, 0xabc, 2, 3, &input).is_err());
        let observation = directory.join("layer3.obs");
        observe(&output, &observation).unwrap();
        let parsed =
            lm_oracle::Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
        assert_eq!(parsed.get("expanded-settings/layer3/file"), Some("2748"));
        assert!(observe(&output, &observation).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
