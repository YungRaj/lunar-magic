use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_graphics::PaletteInterchangeFile;
use lm_oracle::observe_palette;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    validate_paths(input, normalized, observation)?;
    let file = PaletteInterchangeFile::decode(&read_bounded(
        input,
        PaletteInterchangeFile::MAX_FILE_LEN,
    )?)?;
    println!("source-palette: {}", file.source_palette);
    println!("colors: {}", file.palette.colors.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_palette(&file.palette).to_text());
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
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("palette outputs must differ from input");
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized palette and observation outputs must differ");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, Palette};

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("lm-cli-palette-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn outputs_must_be_distinct() {
        let input = Path::new("in");
        let output = Path::new("out");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, None, Some(input)).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());
    }

    #[test]
    fn normalization_and_observation_publish_together() {
        let directory = directory();
        let input = directory.join("input.lmpal");
        let normalized = directory.join("normalized.lmpal");
        let observation = directory.join("palette.obs");
        let file = PaletteInterchangeFile {
            source_palette: 9,
            palette: Palette {
                colors: vec![Bgr555(0), Bgr555(0x001f)],
            },
        };
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            PaletteInterchangeFile::decode(&fs::read(&normalized).unwrap()).unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
        assert_eq!(observed.get("palette/color-count"), Some("2"));
        assert_eq!(observed.get("palette/colors/0001/bgr555"), Some("31"));
        fs::remove_dir_all(directory).unwrap();
    }
}
