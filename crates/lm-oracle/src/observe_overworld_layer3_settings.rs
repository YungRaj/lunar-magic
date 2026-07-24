use crate::{Observation, ObservationError};
use lm_overworld::{
    OVERWORLD_LAYER3_GFX_SLOTS, OVERWORLD_LAYER3_LAYOUT_WORDS, OverworldLayer3SettingsTable,
};
use std::fmt::Write;

/// Produces a semantic oracle for all proven fields in the seven native records.
///
/// # Errors
///
/// Returns an observation error if a generated path collides.
pub fn observe_overworld_layer3_settings(
    table: &OverworldLayer3SettingsTable,
) -> Result<Observation, ObservationError> {
    let mut observation = Observation::new();
    for (map, record) in table.maps.iter().enumerate() {
        let prefix = format!("overworld/layer3/settings/{map}");
        observation.insert(
            format!("{prefix}/feature-flags"),
            format!("{:04x}", record.feature_flags()),
        )?;
        observation.insert(
            format!("{prefix}/custom-tilemap"),
            record.uses_custom_tilemap().to_string(),
        )?;
        observation.insert(
            format!("{prefix}/custom-graphics"),
            record.uses_custom_graphics().to_string(),
        )?;
        observation.insert(
            format!("{prefix}/tilemap-file"),
            format!("{:03x}", record.tilemap_file()),
        )?;
        observation.insert(
            format!("{prefix}/tilemap-size"),
            record.tilemap_size().to_string(),
        )?;
        observation.insert(
            format!("{prefix}/tilemap-position"),
            record.tilemap_position().to_string(),
        )?;
        for index in 0..OVERWORLD_LAYER3_LAYOUT_WORDS {
            let Some(value) = record.address_layout_word(index) else {
                continue;
            };
            observation.insert(
                format!("{prefix}/address-layout/{index}"),
                format!("{value:04x}"),
            )?;
        }
        let preserved = record
            .preserved_bytes()
            .iter()
            .fold(String::new(), |mut output, byte| {
                let _ = write!(output, "{byte:02x}");
                output
            });
        observation.insert(format!("{prefix}/preserved-14-17"), preserved)?;
        for slot in 0..OVERWORLD_LAYER3_GFX_SLOTS {
            let Some(value) = record.graphics_file(slot) else {
                continue;
            };
            observation.insert(
                format!("{prefix}/graphics-file/{slot}"),
                format!("{value:03x}"),
            )?;
        }
    }
    Ok(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_packed_tilemap_semantics() {
        let mut table =
            OverworldLayer3SettingsTable::decode(&[0; OverworldLayer3SettingsTable::ENCODED_LEN])
                .unwrap();
        table.maps[3].set_uses_custom_tilemap(true);
        table.maps[3].set_tilemap_file(0xabc).unwrap();
        table.maps[3].set_tilemap_size(2).unwrap();
        table.maps[3].set_tilemap_position(3).unwrap();
        let observed = observe_overworld_layer3_settings(&table).unwrap();
        assert_eq!(
            observed.get("overworld/layer3/settings/3/tilemap-file"),
            Some("abc")
        );
        assert_eq!(
            observed.get("overworld/layer3/settings/3/tilemap-position"),
            Some("3")
        );
    }
}
