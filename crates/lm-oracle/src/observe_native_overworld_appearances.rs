use crate::{Observation, sha256_hex};
use lm_level::S16OvSidecar;
use lm_overworld::{NativeOverworldSpriteDisplay, NativeOverworldSpriteSidecar};

/// Observes every semantic `.sscov` field and the exact loaded `.s16ov` prefix.
#[must_use]
pub fn observe_native_overworld_appearances(
    definitions: &NativeOverworldSpriteSidecar,
    map16: &S16OvSidecar,
) -> Observation {
    let mut output = Observation::new();
    put(
        &mut output,
        "native-overworld-appearances/tooltip-count",
        definitions.tooltips.len(),
    );
    put(
        &mut output,
        "native-overworld-appearances/appearance-count",
        definitions.appearances.len(),
    );
    for (&sprite_id, tooltip) in &definitions.tooltips {
        let base = format!("native-overworld-appearances/sprites/{sprite_id:03x}/tooltip");
        put(
            &mut output,
            &format!("{base}/disable-position-text"),
            tooltip.disable_original_position_text,
        );
        put(&mut output, &format!("{base}/text"), &tooltip.text);
    }
    for (&sprite_id, appearance) in &definitions.appearances {
        let base = format!("native-overworld-appearances/sprites/{sprite_id:03x}/appearance");
        put(&mut output, &format!("{base}/shadow"), appearance.shadow);
        match &appearance.display {
            NativeOverworldSpriteDisplay::Tiles(parts) => {
                put(&mut output, &format!("{base}/kind"), "tiles");
                put(&mut output, &format!("{base}/part-count"), parts.len());
                for (index, part) in parts.iter().enumerate() {
                    let part_base = format!("{base}/parts/{index:04x}");
                    put(&mut output, &format!("{part_base}/x"), part.x);
                    put(&mut output, &format!("{part_base}/y"), part.y);
                    put(
                        &mut output,
                        &format!("{part_base}/tile"),
                        format!("{:03x}", part.tile),
                    );
                    put(
                        &mut output,
                        &format!("{part_base}/translucent"),
                        part.translucent,
                    );
                }
            }
            NativeOverworldSpriteDisplay::Label { x, y, text } => {
                put(&mut output, &format!("{base}/kind"), "label");
                put(&mut output, &format!("{base}/x"), x);
                put(&mut output, &format!("{base}/y"), y);
                put(&mut output, &format!("{base}/text"), text);
            }
        }
    }
    observe_ranges(&mut output, "graphics", &definitions.graphics_ranges);
    observe_ranges(&mut output, "palette", &definitions.palette_ranges);
    let encoded = map16.encode();
    put(
        &mut output,
        "native-overworld-appearances/map16/loaded-length",
        map16.loaded_len(),
    );
    put(
        &mut output,
        "native-overworld-appearances/map16/sha256",
        sha256_hex(&encoded),
    );
    let nonzero: Vec<_> = (0..S16OvSidecar::ENTRY_COUNT)
        .filter_map(|index| {
            map16
                .entry(index)
                .filter(|value| *value != 0)
                .map(|value| (index, value))
        })
        .collect();
    put(
        &mut output,
        "native-overworld-appearances/map16/nonzero-count",
        nonzero.len(),
    );
    for (index, value) in nonzero {
        put(
            &mut output,
            &format!("native-overworld-appearances/map16/entries/{index:04x}"),
            format!("{value:08x}"),
        );
    }
    output
}

fn observe_ranges(
    output: &mut Observation,
    name: &str,
    ranges: &[lm_overworld::NativeOverworldSpriteRange],
) {
    let base = format!("native-overworld-appearances/{name}-ranges");
    put(output, &format!("{base}/count"), ranges.len());
    for (index, range) in ranges.iter().enumerate() {
        let item = format!("{base}/{index:04x}");
        put(
            output,
            &format!("{item}/kind"),
            format!("{:04x}", range.kind),
        );
        put(
            output,
            &format!("{item}/first"),
            format!("{:03x}", range.first_tile),
        );
        put(
            output,
            &format!("{item}/last"),
            format!("{:03x}", range.last_tile),
        );
        put(
            output,
            &format!("{item}/base"),
            format!("{:04x}", range.base),
        );
    }
}

fn put(output: &mut Observation, path: &str, value: impl ToString) {
    output
        .insert(path, value.to_string())
        .expect("native overworld appearance observation paths are unique");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_retains_native_only_fields_order_and_map16_prefix() {
        let definitions = NativeOverworldSpriteSidecar::decode(
            b"05\t1\tTip\n05\t3\t-2,4,8400 8,9,C01\n06\t2\t7,-8,*Label*\n10000\t12\t400-4FF,1234\n",
        )
        .unwrap();
        let map16 = S16OvSidecar::decode(&[1, 0, 0, 0, 2]).unwrap();
        let observed = observe_native_overworld_appearances(&definitions, &map16);
        assert_eq!(
            observed.get("native-overworld-appearances/sprites/005/appearance/shadow"),
            Some("true")
        );
        assert_eq!(
            observed
                .get("native-overworld-appearances/sprites/005/appearance/parts/0000/translucent"),
            Some("true")
        );
        assert_eq!(
            observed.get("native-overworld-appearances/sprites/006/appearance/text"),
            Some("Label")
        );
        assert_eq!(
            observed.get("native-overworld-appearances/graphics-ranges/0000/base"),
            Some("1234")
        );
        assert_eq!(
            observed.get("native-overworld-appearances/map16/loaded-length"),
            Some("5")
        );
    }
}
