use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_level::{CustomObjectLibrary, MAX_CUSTOM_OBJECT_SIDECAR_LEN};
use lm_oracle::observe_custom_object_library;
#[cfg(test)]
use std::fs;
use std::path::Path;

pub fn execute(
    data: &Path,
    descriptions: &Path,
    normalized_outputs: Option<(&Path, &Path)>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if data == descriptions {
        return Err("custom-object data and description inputs must differ".into());
    }
    if let Some((output_data, output_descriptions)) = normalized_outputs
        && (output_data == output_descriptions
            || output_data == data
            || output_data == descriptions
            || output_descriptions == data
            || output_descriptions == descriptions)
    {
        return Err("normalized custom-object outputs and inputs must all differ".into());
    }
    if let Some(observation) = observation
        && (observation == data
            || observation == descriptions
            || normalized_outputs
                .is_some_and(|(first, second)| observation == first || observation == second))
    {
        return Err("custom-object observation must differ from every sidecar path".into());
    }

    let library = CustomObjectLibrary::decode(
        &read_bounded(data, MAX_CUSTOM_OBJECT_SIDECAR_LEN)?,
        &read_bounded(descriptions, MAX_CUSTOM_OBJECT_SIDECAR_LEN)?,
    )?;
    println!("custom-objects: {}", library.entries().len());
    println!(
        "description-framing: {:?}, bom={}, trailing={}",
        library.description_format().line_ending,
        library.description_format().utf8_bom,
        library.description_format().trailing_line_ending
    );
    let normalized = normalized_outputs.map(|_| library.encode()).transpose()?;
    let observed = observation.map(|_| observe_custom_object_library(&library).to_text());
    let mut outputs = Vec::new();
    if let (Some((output_data, output_descriptions)), Some((encoded_data, encoded_descriptions))) =
        (normalized_outputs, normalized.as_ref())
    {
        outputs.push((output_data, encoded_data.as_slice()));
        outputs.push((output_descriptions, encoded_descriptions.as_slice()));
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    fn directory() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lm-custom-objects-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn input_and_output_aliases_fail_before_file_access() {
        let data = Path::new("objects.mw0");
        let text = Path::new("objects.mw0t");
        assert!(execute(data, data, None, None).is_err());
        assert!(execute(data, text, Some((data, Path::new("new.mw0t"))), None).is_err());
        assert!(
            execute(
                data,
                text,
                Some((Path::new("same"), Path::new("same"))),
                None
            )
            .is_err()
        );
        assert!(execute(data, text, None, Some(data)).is_err());
    }

    #[test]
    fn decoded_pair_is_published_as_one_normalized_group() {
        let directory = directory();
        let data = directory.join("objects.mw0");
        let text = directory.join("objects.mw0t");
        let output_data = directory.join("normalized.mw0");
        let output_text = directory.join("normalized.mw0t");
        fs::write(&data, [0, 0, 0, 0, 0, 1, 0, 3, 0xff]).unwrap();
        fs::write(&text, b"Object\n").unwrap();
        let observation = directory.join("objects.obs");
        execute(
            &data,
            &text,
            Some((&output_data, &output_text)),
            Some(&observation),
        )
        .unwrap();
        assert_eq!(
            fs::read(output_data).unwrap(),
            [0, 0, 0, 0, 0, 1, 0, 3, 0xff]
        );
        assert_eq!(fs::read(output_text).unwrap(), b"Object\n");
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(observed.get("custom-objects/count"), Some("1"));
        assert_eq!(
            observed.get("custom-objects/entries/0000/description"),
            Some("Object")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn oversized_sidecar_cannot_publish_any_group_member() {
        let directory = directory();
        let data = directory.join("oversized.mw0");
        let text = directory.join("objects.mw0t");
        let output_data = directory.join("normalized.mw0");
        let output_text = directory.join("normalized.mw0t");
        let observation = directory.join("objects.obs");
        fs::File::create(&data)
            .unwrap()
            .set_len(u64::try_from(MAX_CUSTOM_OBJECT_SIDECAR_LEN + 1).unwrap())
            .unwrap();
        fs::write(&text, b"Object\n").unwrap();
        assert!(
            execute(
                &data,
                &text,
                Some((&output_data, &output_text)),
                Some(&observation),
            )
            .is_err()
        );
        for path in [&output_data, &output_text, &observation] {
            assert!(!path.exists());
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
