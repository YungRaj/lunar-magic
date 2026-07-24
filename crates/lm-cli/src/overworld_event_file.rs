use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_event_reveals;
use lm_overworld::EventRevealTable;
use std::path::Path;

const MAX_FILE_LEN: usize = 10 + EventRevealTable::MAX_ENTRIES * 4;

pub(crate) fn execute(
    input: &Path,
    normalized: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    if [normalized, observation]
        .into_iter()
        .flatten()
        .any(|path| path == input)
    {
        return Err("overworld-event outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized overworld events and observation outputs must differ".into());
    }
    let table = EventRevealTable::decode_native_event_file(&read_bounded(input, MAX_FILE_LEN)?)?;
    let encoded = normalized
        .map(|_| table.encode_native_event_file())
        .transpose()?;
    let observed = observation
        .map(|_| observe_event_reveals(&table).map(|value| value.to_text()))
        .transpose()?;
    let mut outputs = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized, encoded.as_ref()) {
        outputs.push((path, bytes.as_slice()));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    println!("overworld-event-reveals: {}", table.entries.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::EventReveal;
    use std::fs;

    #[test]
    fn normalizes_and_observes_both_planes_atomically() {
        let directory =
            std::env::temp_dir().join(format!("lm-cli-event-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("events.lmevt");
        let normalized = directory.join("normalized.lmevt");
        let observation = directory.join("events.obs");
        let table = EventRevealTable {
            entries: vec![EventReveal {
                source_tile: 0x123,
                destination_tile: 0x456,
            }],
        };
        fs::write(&input, table.encode_native_event_file().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            EventRevealTable::decode_native_event_file(&fs::read(normalized).unwrap()).unwrap(),
            table
        );
        let observation =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(
            observation.get("overworld/event-reveals/000/source"),
            Some("0123")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_and_aliased_outputs_publish_nothing() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());

        let directory =
            std::env::temp_dir().join(format!("lm-cli-event-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let malformed = directory.join("bad");
        let normalized = directory.join("normalized");
        let observation = directory.join("observation");
        fs::write(&malformed, b"bad").unwrap();
        assert!(execute(&malformed, Some(&normalized), Some(&observation)).is_err());
        assert!(!normalized.exists());
        assert!(!observation.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
