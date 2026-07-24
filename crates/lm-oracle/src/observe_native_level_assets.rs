use crate::{
    Observation, observe_compact_exanimation, observe_expanded_settings, observe_native_level,
    observe_palette,
};
use lm_level::NativeLevelFile;
use lm_project::NativeLevelAssetsFile;

/// Produces a field-complete observation of one decoded `LMNATAS1` aggregate.
#[must_use]
pub fn observe_native_level_assets(file: &NativeLevelAssetsFile) -> Observation {
    let mut result = Observation::new();
    put(&mut result, "native-assets/source-slot", &file.source_slot);
    put(
        &mut result,
        "native-assets/expanded-settings-present",
        &file.assets.expanded_settings.is_some(),
    );
    merge(
        &mut result,
        "native-assets",
        &observe_native_level(&NativeLevelFile {
            source_level: file.source_slot,
            layer1: file.assets.level.layer1.clone(),
            sprites: file.assets.level.sprites.clone(),
        }),
    );
    merge(
        &mut result,
        "native-assets",
        &observe_palette(&file.assets.palette),
    );
    merge(
        &mut result,
        "native-assets",
        &observe_compact_exanimation(&file.assets.exanimation),
    );
    if let Some(settings) = &file.assets.expanded_settings {
        merge(
            &mut result,
            "native-assets",
            &observe_expanded_settings(settings),
        );
    }
    result
}

fn merge(result: &mut Observation, prefix: &str, source: &Observation) {
    for (path, value) in source.entries() {
        result
            .insert(format!("{prefix}/{path}"), value)
            .expect("nested observation paths are unique");
    }
}

fn put(result: &mut Observation, path: &str, value: &impl ToString) {
    result
        .insert(path, value.to_string())
        .expect("native-assets observation paths are unique");
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, Palette};
    use lm_level::{
        ExpandedLevelSettingsRecord, LevelObjectData, NativeSpriteStream, SpriteLengthTable,
    };
    use lm_project::{LoadedLevelSlot, LoadedNativeLevelAssets};

    fn file() -> NativeLevelAssetsFile {
        NativeLevelAssetsFile {
            source_slot: 0x105,
            assets: LoadedNativeLevelAssets {
                level: LoadedLevelSlot {
                    number: 0x105,
                    layer1: LevelObjectData::parse(&[1, 2, 3, 4, 5, 6, 7, 8, 0xff]).unwrap(),
                    sprites: NativeSpriteStream::parse(
                        &[0x10, 0, 1, 2, 0xff],
                        false,
                        &SpriteLengthTable::standard(),
                    )
                    .unwrap(),
                },
                palette: Palette {
                    colors: vec![Bgr555(1), Bgr555(2)],
                },
                exanimation: CompactExAnimation {
                    setting: 7,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: Vec::new(),
                },
                expanded_settings: Some(ExpandedLevelSettingsRecord::decode(&[0x5a; 32]).unwrap()),
            },
        }
    }

    #[test]
    fn observation_composes_every_nested_domain_and_round_trips() {
        let observed = observe_native_level_assets(&file());
        assert_eq!(observed.get("native-assets/source-slot"), Some("261"));
        assert_eq!(
            observed.get("native-assets/native-level/layer1/object-count"),
            Some("1")
        );
        assert_eq!(
            observed.get("native-assets/palette/colors/0001/bgr555"),
            Some("2")
        );
        assert_eq!(observed.get("native-assets/exanimation/setting"), Some("7"));
        assert_eq!(
            observed.get("native-assets/expanded-settings/words/00"),
            Some("23130")
        );
        assert_eq!(
            Observation::from_text(&observed.to_text()).unwrap(),
            observed
        );
    }

    #[test]
    fn one_nested_change_has_a_domain_addressable_difference() {
        let before = file();
        let mut after = before.clone();
        after.assets.palette.colors[1] = Bgr555(9);
        let differences =
            observe_native_level_assets(&before).differences(&observe_native_level_assets(&after));
        assert_eq!(differences.len(), 1);
        assert_eq!(
            differences[0].path,
            "native-assets/palette/colors/0001/bgr555"
        );
    }
}
