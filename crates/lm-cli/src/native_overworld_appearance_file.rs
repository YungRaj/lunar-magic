use crate::{atomic_output::write_new_batch, command_types::Command, oracle_input::read_bounded};
use lm_level::S16OvSidecar;
use lm_overworld::{NativeOverworldSpriteSidecar, SSCOV_MAX_BYTES};
use std::path::Path;

pub fn execute_command(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    let Command::NativeOverworldAppearanceFile {
        definitions,
        sprite_map16,
        normalized_outputs,
        observation,
    } = command
    else {
        return Ok(false);
    };
    execute(
        definitions,
        sprite_map16,
        normalized_outputs
            .as_ref()
            .map(|(first, second)| (first.as_path(), second.as_path())),
        observation.as_deref(),
    )?;
    Ok(true)
}

fn execute(
    definitions_path: &Path,
    map16_path: &Path,
    normalized: Option<(&Path, &Path)>,
    observation: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut paths = vec![definitions_path, map16_path];
    if let Some((first, second)) = normalized {
        paths.extend([first, second]);
    }
    if let Some(path) = observation {
        paths.push(path);
    }
    for (index, path) in paths.iter().enumerate() {
        if paths[..index].contains(path) {
            return Err(
                "native overworld appearance input and output paths must all differ".into(),
            );
        }
    }
    let definitions =
        NativeOverworldSpriteSidecar::decode(&read_bounded(definitions_path, SSCOV_MAX_BYTES)?)?;
    let map16 = S16OvSidecar::decode(&read_bounded(map16_path, S16OvSidecar::CAPACITY)?)?;
    println!("tooltips: {}", definitions.tooltips.len());
    println!("appearances: {}", definitions.appearances.len());
    println!("sprite-map16-bytes: {}", map16.loaded_len());
    let encoded_definitions = normalized.map(|_| definitions.encode()).transpose()?;
    let encoded_map16 = normalized.map(|_| map16.encode());
    let observed = observation
        .map(|_| lm_oracle::observe_native_overworld_appearances(&definitions, &map16).to_text());
    let mut outputs = Vec::new();
    if let (Some((first, second)), Some(definitions), Some(map16)) = (
        normalized,
        encoded_definitions.as_ref(),
        encoded_map16.as_ref(),
    ) {
        outputs.push((first, definitions.as_slice()));
        outputs.push((second, map16.as_slice()));
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
    fn native_pair_normalization_and_observation_publish_atomically() {
        let directory = std::env::temp_dir().join(format!(
            "lm-native-overworld-app-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let definitions = directory.join("sprites.sscov");
        let map16 = directory.join("sprites.s16ov");
        let normalized_definitions = directory.join("normalized.sscov");
        let normalized_map16 = directory.join("normalized.s16ov");
        let observation = directory.join("sprites.obs");
        fs::write(
            &definitions,
            b"\xEF\xBB\xBF05\t1\tTip\r\n05\t3\t-2,4,8400\r\n",
        )
        .unwrap();
        fs::write(&map16, [1, 0, 0, 0, 2]).unwrap();
        execute(
            &definitions,
            &map16,
            Some((&normalized_definitions, &normalized_map16)),
            Some(&observation),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(normalized_definitions).unwrap(),
            "05\t1\tTip\n05\t3\t-2,4,8400\n"
        );
        assert_eq!(fs::read(normalized_map16).unwrap(), [1, 0, 0, 0, 2]);
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(
            observed.get("native-overworld-appearances/map16/loaded-length"),
            Some("5")
        );
        assert!(
            execute(
                &definitions,
                &map16,
                Some((&definitions, Path::new("new.s16ov"))),
                None
            )
            .is_err()
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
