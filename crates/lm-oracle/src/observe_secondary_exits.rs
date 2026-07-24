use crate::{Observation, ObservationError, sha256_hex};
use lm_level::{SecondaryExit, SecondaryExitEncodingError, SecondaryExitTable};

/// Observes the complete logical table independently of fixed versus RATS plane storage.
///
/// # Errors
///
/// Returns a table-encoding error for unrepresentable fields or an observation construction error.
pub fn observe_secondary_exit_table(
    table: &SecondaryExitTable,
) -> Result<Observation, ObserveSecondaryExitError> {
    let encoded = table.encode()?;
    let active: Vec<_> = table
        .entries
        .iter()
        .enumerate()
        .filter(|(_, exit)| is_active(exit))
        .collect();
    let mut observation = Observation::new();
    observation.insert("secondary-exits/active-count", active.len().to_string())?;
    observation.insert(
        "secondary-exits/highest-active",
        active
            .last()
            .map_or_else(|| "none".to_owned(), |(index, _)| format!("{index:04x}")),
    )?;
    for plane in 0..SecondaryExitTable::PLANE_COUNT {
        let start = plane * SecondaryExitTable::ENTRY_COUNT;
        observation.insert(
            format!("secondary-exits/plane/{plane}/sha256"),
            sha256_hex(&encoded[start..start + SecondaryExitTable::ENTRY_COUNT]),
        )?;
    }
    Ok(observation)
}

fn is_active(exit: &SecondaryExit) -> bool {
    exit.destination_level.to_le_bytes()[0] != 0
        || exit.x_and_overworld_flags & 0x80 != 0
        || exit.destination_flags & 0x40 != 0
        || exit.position_and_method & 0x0f != 0
        || exit.screen != 0
        || exit.y != 0
}

#[derive(Debug)]
pub enum ObserveSecondaryExitError {
    Table(SecondaryExitEncodingError),
    Observation(ObservationError),
}

impl std::fmt::Display for ObserveSecondaryExitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "secondary-exit observation failed: {self:?}")
    }
}

impl std::error::Error for ObserveSecondaryExitError {}

impl From<SecondaryExitEncodingError> for ObserveSecondaryExitError {
    fn from(value: SecondaryExitEncodingError) -> Self {
        Self::Table(value)
    }
}

impl From<ObservationError> for ObserveSecondaryExitError {
    fn from(value: ObservationError) -> Self {
        Self::Observation(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_high_flags_do_not_invent_an_exit_but_overworld_flag_does() {
        let mut entries = vec![SecondaryExit::default(); SecondaryExitTable::ENTRY_COUNT];
        entries[7].additional_flags = 0x20;
        let first = observe_secondary_exit_table(&SecondaryExitTable {
            entries: entries.clone(),
        })
        .unwrap();
        assert_eq!(first.get("secondary-exits/active-count"), Some("0"));
        entries[7].x_and_overworld_flags = 0x80;
        let second = observe_secondary_exit_table(&SecondaryExitTable { entries }).unwrap();
        assert_eq!(second.get("secondary-exits/active-count"), Some("1"));
        assert_eq!(second.get("secondary-exits/highest-active"), Some("0007"));
    }
}
