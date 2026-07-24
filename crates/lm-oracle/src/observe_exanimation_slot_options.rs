use crate::{Observation, ObservationError};
use lm_graphics::ExAnimationSlotOptionTable;

/// Observes the semantic and preserved fields of all seven `ExAnimation` slot options.
///
/// # Errors
///
/// Returns an observation error if a generated semantic path collides.
pub fn observe_exanimation_slot_options(
    table: &ExAnimationSlotOptionTable,
) -> Result<Observation, ObservationError> {
    let mut result = Observation::new();
    for (slot, options) in table.slots.iter().enumerate() {
        let prefix = format!("exanimation/slot-options/{slot}");
        result.insert(
            format!("{prefix}/preserved-low-nibble"),
            format!("{:x}", options.preserved_low_nibble),
        )?;
        for (option, value) in options.enabled.iter().enumerate() {
            result.insert(
                format!("{prefix}/bit{}-enabled", option + 4),
                enabled(*value),
            )?;
        }
    }
    Ok(result)
}

const fn enabled(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::ExAnimationSlotOptions;

    #[test]
    fn inverted_options_are_exposed_as_positive_booleans() {
        let table = ExAnimationSlotOptionTable {
            slots: [ExAnimationSlotOptions {
                preserved_low_nibble: 3,
                enabled: [true, false, true, false],
            }; 7],
        };
        let observed = observe_exanimation_slot_options(&table).unwrap();
        assert_eq!(
            observed.get("exanimation/slot-options/0/preserved-low-nibble"),
            Some("3")
        );
        assert_eq!(
            observed.get("exanimation/slot-options/0/bit4-enabled"),
            Some("true")
        );
        assert_eq!(
            observed.get("exanimation/slot-options/0/bit7-enabled"),
            Some("false")
        );
    }
}
