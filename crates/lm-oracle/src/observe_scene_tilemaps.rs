use crate::{Observation, ObservationError, sha256_hex};
use lm_overworld::{CreditsTilemap, ExpandedLayerTilemap};

/// Produces independently addressable row hashes for the complete credits model.
///
/// # Errors
///
/// Returns an observation construction error if a semantic path is duplicated.
pub fn observe_credits_tilemap(tilemap: &CreditsTilemap) -> Result<Observation, ObservationError> {
    let mut observation = Observation::new();
    let bytes: Vec<_> = tilemap
        .words()
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    observation.insert("credits/tilemap/sha256", sha256_hex(&bytes))?;
    for (row, words) in tilemap
        .words()
        .chunks_exact(CreditsTilemap::COLUMNS)
        .enumerate()
    {
        let row_bytes: Vec<_> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
        observation.insert(
            format!("credits/tilemap/row/{row}/sha256"),
            sha256_hex(&row_bytes),
        )?;
    }
    Ok(observation)
}

/// Produces allocation- and optional-plane-framing-independent layer hashes.
///
/// # Errors
///
/// Returns an observation construction error if a semantic path is duplicated.
pub fn observe_expanded_layer_tilemap(
    tilemap: &ExpandedLayerTilemap,
) -> Result<Observation, ObservationError> {
    let mut observation = Observation::new();
    observation.insert(
        "scene/layer-tilemap/primary-sha256",
        sha256_hex(tilemap.primary_bytes()),
    )?;
    observation.insert(
        "scene/layer-tilemap/secondary-sha256",
        sha256_hex(tilemap.secondary_bytes()),
    )?;
    observation.insert(
        "scene/layer-tilemap/secondary-blank",
        tilemap.secondary_is_blank().to_string(),
    )?;
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_rows_and_layer_planes_are_independently_addressable() {
        let baseline = CreditsTilemap::blank(0x38fc);
        let first = observe_credits_tilemap(&baseline).unwrap();
        let mut changed = baseline;
        changed.words_mut()[CreditsTilemap::COLUMNS + 1] ^= 1;
        let differences = first.differences(&observe_credits_tilemap(&changed).unwrap());
        assert!(
            differences
                .iter()
                .any(|difference| { difference.path == "credits/tilemap/row/1/sha256" })
        );
        assert!(
            !differences
                .iter()
                .any(|difference| { difference.path == "credits/tilemap/row/0/sha256" })
        );
    }
}
