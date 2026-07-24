use crate::{Observation, ObservationError};

/// Observes Lunar Magic's decoded transferred Map16 definitions and acts-like values.
///
/// Compression packets, split pointers, trimming, and allocation addresses are deliberately
/// excluded so equivalent relocated encodings compare by editor meaning.
///
/// # Errors
///
/// Returns an observation error if a generated semantic path collides.
pub fn observe_transferred_map16(
    definitions: &[u16],
    acts_like: &[u16],
) -> Result<Observation, ObservationError> {
    let mut result = Observation::new();
    result.insert(
        "map16/transferred/definition-words",
        definitions.len().to_string(),
    )?;
    for (index, word) in definitions.iter().enumerate() {
        result.insert(
            format!("map16/transferred/definitions/{index:04x}"),
            format!("{word:04x}"),
        )?;
    }
    result.insert(
        "map16/transferred/acts-like-count",
        acts_like.len().to_string(),
    )?;
    for (index, word) in acts_like.iter().enumerate() {
        result.insert(
            format!("map16/transferred/acts-like/{index:04x}"),
            format!("{word:04x}"),
        )?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_is_independently_addressable() {
        let observation = observe_transferred_map16(&[0x1234, 0xabcd], &[0x0001]).unwrap();
        assert_eq!(
            observation.get("map16/transferred/definitions/0001"),
            Some("abcd")
        );
        assert_eq!(
            observation.get("map16/transferred/acts-like/0000"),
            Some("0001")
        );
    }
}
