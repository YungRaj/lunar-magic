use crate::{Observation, ObservationError};
use lm_overworld::EventRevealTable;

/// Produces an entry-addressable observation of the native main event-reveal workspace.
///
/// # Errors
///
/// Returns an observation error if a generated semantic path is duplicated.
pub fn observe_event_reveals(table: &EventRevealTable) -> Result<Observation, ObservationError> {
    let mut observation = Observation::new();
    observation.insert(
        "overworld/event-reveals/count",
        table.entries.len().to_string(),
    )?;
    for (index, entry) in table.entries.iter().enumerate() {
        let base = format!("overworld/event-reveals/{index:03x}");
        observation.insert(
            format!("{base}/source"),
            format!("{:04x}", entry.source_tile),
        )?;
        observation.insert(
            format!("{base}/destination"),
            format!("{:04x}", entry.destination_tile),
        )?;
    }
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::EventReveal;

    #[test]
    fn count_and_both_planes_are_independently_addressable() {
        let table = EventRevealTable {
            entries: vec![EventReveal {
                source_tile: 0x123,
                destination_tile: 0x456,
            }],
        };
        let observation = observe_event_reveals(&table).unwrap();
        assert_eq!(observation.get("overworld/event-reveals/count"), Some("1"));
        assert_eq!(
            observation.get("overworld/event-reveals/000/source"),
            Some("0123")
        );
        assert_eq!(
            observation.get("overworld/event-reveals/000/destination"),
            Some("0456")
        );
    }
}
