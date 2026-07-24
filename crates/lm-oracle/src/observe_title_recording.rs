use crate::{Observation, ObservationError, sha256_hex};
use lm_title::TitleScreenRecording;

/// Produces allocation- and savestate-container-independent recording evidence.
///
/// # Errors
///
/// Returns an observation construction error if a semantic path is duplicated.
pub fn observe_title_recording(
    recording: &TitleScreenRecording,
) -> Result<Observation, ObservationError> {
    let mut observation = Observation::new();
    observation.insert(
        "title/recording/length",
        recording.bytes().len().to_string(),
    )?;
    observation.insert("title/recording/sha256", sha256_hex(recording.bytes()))?;
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_changes_are_observable_without_container_state() {
        let first = TitleScreenRecording::from_bytes(vec![1, 2, 3, 0xff]).unwrap();
        let second = TitleScreenRecording::from_bytes(vec![1, 2, 4, 0xff]).unwrap();
        assert_ne!(
            observe_title_recording(&first).unwrap(),
            observe_title_recording(&second).unwrap()
        );
    }
}
