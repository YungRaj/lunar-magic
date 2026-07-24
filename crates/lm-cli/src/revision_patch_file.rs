use crate::{atomic_output::write_new_batch, oracle_input::read_bounded};
use lm_oracle::observe_revision_patch;
use lm_profile::RevisionPatchTemplate;
use std::path::Path;

pub(crate) fn execute(
    input: &Path,
    normalized_output: Option<&Path>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    for output in [normalized_output, observation].into_iter().flatten() {
        if output == input {
            return Err("revision patch outputs must differ from input".into());
        }
    }
    if normalized_output.is_some() && normalized_output == observation {
        return Err("revision patch normalized and observation outputs must differ".into());
    }
    let template =
        RevisionPatchTemplate::decode(&read_bounded(input, RevisionPatchTemplate::MAX_FILE_LEN)?)?;
    let normalized = normalized_output.map(|_| template.encode()).transpose()?;
    let observed = observation.map(|_| observe_revision_patch(&template).to_text());
    let mut outputs: Vec<(&Path, &[u8])> = Vec::new();
    if let (Some(path), Some(bytes)) = (normalized_output, normalized.as_ref()) {
        outputs.push((path, bytes));
    }
    if let (Some(path), Some(text)) = (observation, observed.as_ref()) {
        outputs.push((path, text.as_bytes()));
    }
    write_new_batch(&outputs)?;
    println!(
        "revision-patch: {} payloads={} writes={}",
        template.name,
        template.payloads.len(),
        template.writes.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::{PatchFixup, PatchPayload, PatchWrite};
    use lm_rom::{Mapper, Region, SupportedGame};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn path(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lm-revision-patch-file-{}-{nonce}-{name}",
            std::process::id()
        ))
    }

    fn template() -> RevisionPatchTemplate {
        RevisionPatchTemplate {
            name: "clean room runtime".into(),
            game: SupportedGame::SuperMarioWorld,
            region: Region::NorthAmerica,
            revision: 0,
            mapper: Mapper::LoRom,
            payloads: vec![PatchPayload {
                bytes: vec![1, 2, 3, 4],
                fixups: Vec::new(),
            }],
            writes: vec![PatchWrite {
                offset: 0x100,
                expected: vec![0xea; 4],
                replacement: vec![0x22, 0, 0, 0],
                fixups: vec![PatchFixup {
                    offset: 1,
                    target_payload: 0,
                    target_addend: 0,
                    encoding: lm_project::PatchFixupEncoding::Long24,
                }],
            }],
        }
    }

    #[test]
    fn normalization_and_observation_are_atomic_and_create_new() {
        let input = path("input.lmpatch");
        let normalized = path("normalized.lmpatch");
        let observation = path("template.obs");
        let encoded = template().encode().unwrap();
        fs::write(&input, &encoded).unwrap();

        execute(&input, Some(&normalized), Some(&observation)).unwrap();

        assert_eq!(fs::read(&normalized).unwrap(), encoded);
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(&observation).unwrap()).unwrap();
        assert_eq!(observed.get("revision-patch/payload-count"), Some("1"));
        assert!(execute(&input, Some(&normalized), None).is_err());
        assert!(execute(&input, Some(&input), None).is_err());
        for path in [input, normalized, observation] {
            fs::remove_file(path).unwrap();
        }
    }
}
