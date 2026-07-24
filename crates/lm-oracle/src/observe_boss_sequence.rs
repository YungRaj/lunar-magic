use crate::{Observation, ObservationError, sha256_hex};
use lm_overworld::BossSequenceMessageTable;

/// Produces an allocation-independent snapshot with each message independently addressable.
///
/// # Errors
///
/// Returns an observation construction error if a semantic path is duplicated.
pub fn observe_boss_sequence_messages(
    table: &BossSequenceMessageTable,
) -> Result<Observation, ObservationError> {
    let mut observation = Observation::new();
    observation.insert(
        "overworld/boss-sequence/message-count",
        BossSequenceMessageTable::MESSAGE_COUNT.to_string(),
    )?;
    let mut aggregate = Vec::with_capacity(
        BossSequenceMessageTable::MESSAGE_COUNT * lm_overworld::BossSequenceMessage::ENCODED_LEN,
    );
    for (index, message) in table.messages.iter().enumerate() {
        aggregate.extend_from_slice(message.encoded());
        observation.insert(
            format!("overworld/boss-sequence/message/{index}/sha256"),
            sha256_hex(message.encoded()),
        )?;
    }
    observation.insert(
        "overworld/boss-sequence/all-messages-sha256",
        sha256_hex(&aggregate),
    )?;
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_one_message_only_changes_its_addressable_hash() {
        let first = BossSequenceMessageTable::default();
        let baseline = observe_boss_sequence_messages(&first).unwrap();
        let mut changed = first;
        changed.messages[3].0[9] = 0x44;
        let differences = baseline.differences(&observe_boss_sequence_messages(&changed).unwrap());
        assert!(
            differences.iter().any(|difference| {
                difference.path == "overworld/boss-sequence/message/3/sha256"
            })
        );
        assert!(
            !differences.iter().any(|difference| {
                difference.path == "overworld/boss-sequence/message/2/sha256"
            })
        );
    }
}
