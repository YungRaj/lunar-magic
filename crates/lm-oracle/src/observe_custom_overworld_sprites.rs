use crate::{Observation, ObservationError};
use lm_overworld::NativeCustomOverworldSpriteTable;

/// Observes native custom overworld sprites by map and record index.
///
/// Offset aliases, terminators, RATS placement, and packed bit spelling are deliberately omitted.
///
/// # Errors
///
/// Returns an observation error if a generated semantic path collides.
pub fn observe_custom_overworld_sprites(
    table: &NativeCustomOverworldSpriteTable,
) -> Result<Observation, ObservationError> {
    let mut result = Observation::new();
    for (map, records) in table.maps.iter().enumerate() {
        result.insert(
            format!("overworld/custom-sprites/maps/{map}/count"),
            records.len().to_string(),
        )?;
        for (index, record) in records.iter().enumerate() {
            let prefix = format!("overworld/custom-sprites/maps/{map}/{index:02x}");
            result.insert(format!("{prefix}/id"), format!("{:02x}", record.id))?;
            result.insert(format!("{prefix}/x"), record.x.to_string())?;
            result.insert(format!("{prefix}/y"), record.y.to_string())?;
            result.insert(format!("{prefix}/screen"), record.screen.to_string())?;
            result.insert(format!("{prefix}/extra"), hex(&record.extra))?;
        }
    }
    Ok(result)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0xf)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_overworld::NativeCustomOverworldSprite;

    #[test]
    fn every_semantic_field_is_independently_addressable() {
        let table = NativeCustomOverworldSpriteTable {
            maps: std::array::from_fn(|map| {
                if map == 3 {
                    vec![NativeCustomOverworldSprite {
                        id: 7,
                        x: 16,
                        y: 24,
                        screen: 8,
                        extra: vec![0xab, 0xcd],
                    }]
                } else {
                    Vec::new()
                }
            }),
        };
        let observed = observe_custom_overworld_sprites(&table).unwrap();
        assert_eq!(
            observed.get("overworld/custom-sprites/maps/3/00/id"),
            Some("07")
        );
        assert_eq!(
            observed.get("overworld/custom-sprites/maps/3/00/extra"),
            Some("abcd")
        );
    }
}
