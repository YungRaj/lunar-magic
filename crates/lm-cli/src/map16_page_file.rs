use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_exact;
use lm_level::Map16PageFile;
use lm_oracle::observe_map16_page_file;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("Map16 page outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized Map16 page and observation outputs must differ".into());
    }
    let file = Map16PageFile::decode(&read_exact(
        input,
        Map16PageFile::ENCODED_LEN,
        "Map16 page file",
    )?)?;
    println!("source-page: {:#04x}", file.source_page);
    println!("tiles: {}", file.page.tiles.len());
    let encoded = normalized.map(|_| file.encode()).transpose()?;
    let observed = observation.map(|_| observe_map16_page_file(&file).to_text());
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

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::{Map16Page, Map16Tile};
    #[test]
    fn outputs_are_distinct_and_page_identity_is_observed() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());
        let directory =
            std::env::temp_dir().join(format!("lm-cli-map16-page-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("page.map16");
        let normalized = directory.join("normalized.map16");
        let observation = directory.join("page.obs");
        let file = Map16PageFile {
            source_page: 0x42,
            page: Map16Page::new(vec![Map16Tile::default(); 256]).unwrap(),
        };
        fs::write(&input, file.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            Map16PageFile::decode(&fs::read(normalized).unwrap()).unwrap(),
            file
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(observed.get("map16/source-page"), Some("66"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn oversized_exact_page_is_rejected_without_outputs() {
        let directory =
            std::env::temp_dir().join(format!("lm-cli-map16-page-bound-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("oversized.map16");
        let normalized = directory.join("normalized.map16");
        fs::File::create(&input)
            .unwrap()
            .set_len(u64::try_from(Map16PageFile::ENCODED_LEN + 1).unwrap())
            .unwrap();
        assert!(execute(&input, Some(&normalized), None).is_err());
        assert!(!normalized.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
