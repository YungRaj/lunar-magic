use crate::{Observation, ObservationError, sha256_hex};
use lm_overworld::EventTilemapBuffers;

/// Produces a compression- and allocation-independent snapshot of both owned event streams.
///
/// # Errors
///
/// Returns an observation construction error if a semantic path is duplicated.
pub fn observe_event_tilemap_buffers(
    buffers: &EventTilemapBuffers,
) -> Result<Observation, ObservationError> {
    let primary = buffers.primary_bytes();
    let secondary = buffers.secondary_high_bytes();
    let mut observation = Observation::new();
    observation.insert(
        "overworld/event-tilemap/primary-bytes",
        primary.len().to_string(),
    )?;
    observation.insert(
        "overworld/event-tilemap/primary-sha256",
        sha256_hex(primary),
    )?;
    observation.insert(
        "overworld/event-tilemap/index-plane-sha256",
        sha256_hex(&primary[..EventTilemapBuffers::WORD_COUNT]),
    )?;
    observation.insert(
        "overworld/event-tilemap/auxiliary-plane-sha256",
        sha256_hex(&primary[EventTilemapBuffers::WORD_COUNT..]),
    )?;
    observation.insert(
        "overworld/event-tilemap/secondary-high-bytes",
        secondary.len().to_string(),
    )?;
    observation.insert(
        "overworld/event-tilemap/secondary-high-sha256",
        sha256_hex(secondary),
    )?;
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_native_plane_is_independently_addressable() {
        let mut first = EventTilemapBuffers::default();
        let baseline = observe_event_tilemap_buffers(&first).unwrap();
        first.primary_bytes_mut()[0] = 1;
        let index_changed = observe_event_tilemap_buffers(&first).unwrap();
        let differences = baseline.differences(&index_changed);
        assert!(
            differences.iter().any(|difference| {
                difference.path == "overworld/event-tilemap/index-plane-sha256"
            })
        );
        assert!(!differences.iter().any(|difference| {
            difference.path == "overworld/event-tilemap/auxiliary-plane-sha256"
        }));
    }
}
