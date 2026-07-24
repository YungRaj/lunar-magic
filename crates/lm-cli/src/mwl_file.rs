use crate::{atomic_output::write_new, oracle_input::read_bounded};
use lm_level::MwlFile;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn normalize(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("MWL input and output paths must differ".into());
    }
    let normalized = MwlFile::decode(&read_bounded(input, MwlFile::MAX_FILE_BYTES)?)?.encode()?;
    write_new(output, normalized)?;
    println!("normalized-mwl: {}", output.display());
    Ok(())
}

pub fn observe(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if input == output {
        return Err("MWL input and observation paths must differ".into());
    }
    let file = MwlFile::decode(&read_bounded(input, MwlFile::MAX_FILE_BYTES)?)?;
    write_new(output, lm_oracle::observe_mwl(&file).to_text())?;
    println!("observed-mwl: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::MwlSectionKind;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-mwl-{}-{}-{name}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn normalization_is_semantic_and_create_new() {
        let input = path("input.mwl");
        let output = path("output.mwl");
        let mut file = MwlFile::default();
        file.set_section(MwlSectionKind::LevelHeader, vec![3; 0x40]);
        file.set_section(MwlSectionKind::Layer1, vec![1, 2, 3]);
        fs::write(&input, file.encode().unwrap()).unwrap();
        normalize(&input, &output).unwrap();
        assert_eq!(MwlFile::decode(&fs::read(&output).unwrap()).unwrap(), file);
        assert!(normalize(&input, &output).is_err());
        assert!(normalize(&input, &input).is_err());
        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }

    #[test]
    fn malformed_input_never_publishes_output() {
        let input = path("bad.mwl");
        let output = path("absent.mwl");
        fs::write(&input, b"LM").unwrap();
        assert!(normalize(&input, &output).is_err());
        assert!(!output.exists());
        fs::remove_file(input).unwrap();
    }

    #[test]
    fn observation_is_canonical_and_create_new() {
        let input = path("observed.mwl");
        let output = path("level.obs");
        let mut file = MwlFile::default();
        file.set_section(MwlSectionKind::Sprites, vec![0; 8]);
        fs::write(&input, file.encode().unwrap()).unwrap();
        observe(&input, &output).unwrap();
        let observation = lm_oracle::Observation::from_text(
            std::str::from_utf8(&fs::read(&output).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(observation.get("mwl/sections/sprites/length"), Some("8"));
        assert!(observe(&input, &output).is_err());
        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }
}
