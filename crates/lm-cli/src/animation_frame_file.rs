use crate::atomic_output::write_new_batch;
use crate::oracle_input::read_bounded;
use lm_graphics::MaterializedAnimationFrame;
use lm_oracle::observe_materialized_animation_frame;
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
        return Err("animation frame outputs must differ from input".into());
    }
    if normalized.is_some() && normalized == observation {
        return Err("normalized animation frame and observation outputs must differ".into());
    }
    let frame = MaterializedAnimationFrame::decode(&read_bounded(
        input,
        MaterializedAnimationFrame::MAX_FILE_LEN,
    )?)?;
    println!("tick: {}", frame.tick);
    println!("tile overrides: {}", frame.tile_overrides.len());
    println!("palette overrides: {}", frame.palette_overrides.len());
    let encoded = normalized.map(|_| frame.encode()).transpose()?;
    let observed = observation.map(|_| observe_materialized_animation_frame(&frame).to_text());
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
    use lm_graphics::{Bgr555, MaterializedPaletteOverride};

    #[test]
    fn frame_round_trips_and_observes_atomically() {
        let input = Path::new("input");
        let output = Path::new("output");
        assert!(execute(input, Some(input), None).is_err());
        assert!(execute(input, Some(output), Some(output)).is_err());
        let directory = std::env::temp_dir().join(format!(
            "lm-cli-animation-frame-file-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        let input = directory.join("frame.lmanfrm");
        let normalized = directory.join("normalized.lmanfrm");
        let observation = directory.join("frame.obs");
        let frame = MaterializedAnimationFrame {
            tick: 9,
            tile_overrides: Vec::new(),
            palette_overrides: vec![MaterializedPaletteOverride {
                color_index: 3,
                color: Bgr555(0x4567),
            }],
        };
        fs::write(&input, frame.encode().unwrap()).unwrap();
        execute(&input, Some(&normalized), Some(&observation)).unwrap();
        assert_eq!(
            MaterializedAnimationFrame::decode(&fs::read(normalized).unwrap()).unwrap(),
            frame
        );
        let observed =
            lm_oracle::Observation::from_text(&fs::read_to_string(observation).unwrap()).unwrap();
        assert_eq!(observed.get("animation-frame/tick"), Some("9"));
        fs::remove_dir_all(directory).unwrap();
    }
}
