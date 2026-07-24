use crate::Observation;
use lm_level::{AppearanceSource, EntityAppearanceFile};
use lm_overworld::SpriteAppearanceFile;

/// Produces a painter-order snapshot of provider-resolved level entities.
#[must_use]
pub fn observe_entity_appearances(file: &EntityAppearanceFile) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "entity-appearances/count",
        file.appearances.len(),
    );
    for (position, appearance) in file.appearances.iter().enumerate() {
        let base = format!("entity-appearances/records/{position:06x}");
        let (kind, index) = match appearance.source {
            AppearanceSource::Layer1Object(index) => ("layer1-object", index),
            AppearanceSource::Layer2Object(index) => ("layer2-object", index),
            AppearanceSource::Sprite(index) => ("sprite", index),
        };
        put(&mut result, &format!("{base}/source-kind"), kind);
        put(&mut result, &format!("{base}/source-index"), index);
        put(&mut result, &format!("{base}/tile"), appearance.tile_index);
        put(
            &mut result,
            &format!("{base}/palette"),
            appearance.palette_index,
        );
        put(&mut result, &format!("{base}/x"), appearance.x);
        put(&mut result, &format!("{base}/y"), appearance.y);
        put(&mut result, &format!("{base}/x-flip"), appearance.x_flip);
        put(&mut result, &format!("{base}/y-flip"), appearance.y_flip);
    }
    result
}

/// Produces a sprite-ID-addressable snapshot retaining each definition's part order.
#[must_use]
pub fn observe_overworld_appearances(file: &SpriteAppearanceFile) -> Observation {
    let mut result = Observation::new();
    put(
        &mut result,
        "overworld-appearances/definition-count",
        file.definitions.len(),
    );
    for definition in &file.definitions {
        let base = format!(
            "overworld-appearances/definitions/{:04x}",
            definition.sprite_id
        );
        put(
            &mut result,
            &format!("{base}/part-count"),
            definition.parts.len(),
        );
        for (position, part) in definition.parts.iter().enumerate() {
            let part_base = format!("{base}/parts/{position:04x}");
            put(&mut result, &format!("{part_base}/tile"), part.tile_index);
            put(
                &mut result,
                &format!("{part_base}/palette"),
                part.palette_index,
            );
            put(&mut result, &format!("{part_base}/x"), part.x_offset);
            put(&mut result, &format!("{part_base}/y"), part.y_offset);
            put(&mut result, &format!("{part_base}/x-flip"), part.x_flip);
            put(&mut result, &format!("{part_base}/y-flip"), part.y_flip);
        }
    }
    result
}

fn put(result: &mut Observation, path: &str, value: impl ObservationValue) {
    result
        .insert(path, value.into_value())
        .expect("observation paths are unique");
}

trait ObservationValue {
    fn into_value(self) -> String;
}

impl<T: ToString> ObservationValue for T {
    fn into_value(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_level::EntityAppearanceRecord;
    use lm_overworld::{SpriteAppearanceDefinition, SpriteAppearancePart};

    #[test]
    fn entity_observation_preserves_painter_order_and_signed_positions() {
        let file = EntityAppearanceFile {
            appearances: vec![EntityAppearanceRecord {
                source: AppearanceSource::Layer2Object(7),
                tile_index: 4,
                palette_index: 3,
                x: -12,
                y: 8,
                x_flip: true,
                y_flip: false,
            }],
        };
        let observed = observe_entity_appearances(&file);
        assert_eq!(
            observed.get("entity-appearances/records/000000/source-kind"),
            Some("layer2-object")
        );
        assert_eq!(
            observed.get("entity-appearances/records/000000/x"),
            Some("-12")
        );
    }

    #[test]
    fn overworld_observation_addresses_definitions_by_sprite_id() {
        let file = SpriteAppearanceFile {
            definitions: vec![SpriteAppearanceDefinition {
                sprite_id: 0x1234,
                parts: vec![SpriteAppearancePart {
                    tile_index: 9,
                    palette_index: 2,
                    x_offset: -4,
                    y_offset: 5,
                    x_flip: false,
                    y_flip: true,
                }],
            }],
        };
        let observed = observe_overworld_appearances(&file);
        assert_eq!(
            observed.get("overworld-appearances/definitions/1234/parts/0000/y-flip"),
            Some("true")
        );
    }
}
