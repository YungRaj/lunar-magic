//! Shared loading of bounded aggregate composition scripts.

use crate::{
    editor_shell::read_bounded_utf8, exanimation_edit_script, exanimation_feature_edit_script,
    expanded_settings_edit_script, layer2_object_edit_script, layer2_tilemap_edit_script,
    level_edit_script, map16_edit_script, native_assets_edit_spec, palette_edit_script,
    sprite_spawn_edit_script,
};
use lm_app::{Map16ControllerEdit, NativeLevelAssetsControllerEdit};
use lm_graphics::PaletteOwnership;
use std::path::Path;

pub(crate) struct LoadedNativeAssetsEdits {
    pub edits: Vec<NativeLevelAssetsControllerEdit>,
    pub map16_edits: Vec<Map16ControllerEdit>,
    pub palette_ownership: Option<PaletteOwnership>,
}

pub(crate) fn load(path: &Path) -> Result<LoadedNativeAssetsEdits, Box<dyn std::error::Error>> {
    let spec = native_assets_edit_spec::read(path)?;
    let mut edits = Vec::new();
    let map16_edits = if let Some(path) = spec.map16 {
        let text = read_bounded_utf8(&path, map16_edit_script::MAX_SCRIPT_LEN, "Map16 edit")?;
        map16_edit_script::parse(&text)?
    } else {
        Vec::new()
    };
    if let Some(path) = spec.level {
        let text = read_bounded_utf8(&path, level_edit_script::MAX_SCRIPT_LEN, "level edit")?;
        edits.push(NativeLevelAssetsControllerEdit::Level(
            level_edit_script::parse(&text)?,
        ));
    }
    if let Some(path) = spec.layer2_objects {
        let text = read_bounded_utf8(
            &path,
            layer2_object_edit_script::MAX_SCRIPT_LEN,
            "Layer 2 object edit",
        )?;
        edits.push(NativeLevelAssetsControllerEdit::Layer2Objects(
            layer2_object_edit_script::parse(&text)?,
        ));
    }
    if let Some(path) = spec.layer2_tilemap {
        let text = read_bounded_utf8(
            &path,
            layer2_tilemap_edit_script::MAX_SCRIPT_LEN,
            "Layer 2 tilemap edit",
        )?;
        edits.extend(layer2_tilemap_edit_script::parse(&text)?);
    }
    let palette_script = if let Some(path) = spec.palette {
        let text = read_bounded_utf8(&path, palette_edit_script::MAX_SCRIPT_LEN, "palette edit")?;
        Some(palette_edit_script::parse(&text)?)
    } else {
        None
    };
    if let Some(script) = &palette_script {
        edits.push(NativeLevelAssetsControllerEdit::Palette(
            script.edits.clone(),
        ));
    }
    if let Some(path) = spec.exanimation {
        let text = read_bounded_utf8(
            &path,
            exanimation_edit_script::MAX_SCRIPT_LEN,
            "ExAnimation edit",
        )?;
        edits.push(NativeLevelAssetsControllerEdit::ExAnimation(
            exanimation_edit_script::parse(&text)?,
        ));
    }
    if let Some(path) = spec.exanimation_features {
        let text = read_bounded_utf8(
            &path,
            exanimation_feature_edit_script::MAX_SCRIPT_LEN,
            "animation-feature edit",
        )?;
        let edit = exanimation_feature_edit_script::parse(&text)?;
        edits.push(NativeLevelAssetsControllerEdit::ExAnimationFeatureStates {
            palette: edit.palette,
            vanilla: edit.vanilla,
            global: edit.global,
            level: edit.level,
        });
    }
    if let Some(path) = spec.expanded_settings {
        let text = read_bounded_utf8(
            &path,
            expanded_settings_edit_script::MAX_SCRIPT_LEN,
            "expanded-settings edit",
        )?;
        let script = expanded_settings_edit_script::parse(&text)?;
        for edit in script.edits {
            match edit {
                expanded_settings_edit_script::ExpandedSettingsScriptEdit::Word {
                    index,
                    value,
                } => edits.push(NativeLevelAssetsControllerEdit::ExpandedSettingsWords(vec![(
                    index, value,
                )])),
                expanded_settings_edit_script::ExpandedSettingsScriptEdit::Layer3Tilemap {
                    enabled,
                    descriptor,
                } => edits.push(NativeLevelAssetsControllerEdit::Layer3TilemapSettings {
                    enabled,
                    descriptor,
                }),
                expanded_settings_edit_script::ExpandedSettingsScriptEdit::Layer3ExpandedMode(
                    flags,
                ) => edits.push(NativeLevelAssetsControllerEdit::Layer3ExpandedMode(flags)),
                expanded_settings_edit_script::ExpandedSettingsScriptEdit::SuperGraphicsBypass(
                    bypass,
                ) => edits.push(NativeLevelAssetsControllerEdit::SuperGraphicsBypass(bypass)),
                expanded_settings_edit_script::ExpandedSettingsScriptEdit::SpriteBoundaryInteractionAir(
                    enabled,
                ) => edits.push(
                    NativeLevelAssetsControllerEdit::SpriteBoundaryInteractionAir(enabled),
                ),
            }
        }
    }
    if let Some(path) = spec.sprite_spawn {
        let text = read_bounded_utf8(
            &path,
            sprite_spawn_edit_script::MAX_SCRIPT_LEN,
            "sprite-spawn edit",
        )?;
        edits.extend(
            sprite_spawn_edit_script::parse(&text)?
                .into_iter()
                .map(|edit| match edit {
                    sprite_spawn_edit_script::SpriteSpawnEdit::Properties {
                        vertical_range,
                        smart_spawn,
                    } => NativeLevelAssetsControllerEdit::SpriteSpawnProperties {
                        vertical_range,
                        smart_spawn,
                    },
                    sprite_spawn_edit_script::SpriteSpawnEdit::BoundaryInteractionAir(enabled) => {
                        NativeLevelAssetsControllerEdit::SpriteBoundaryInteractionAir(enabled)
                    }
                }),
        );
    }
    Ok(LoadedNativeAssetsEdits {
        edits,
        map16_edits,
        palette_ownership: palette_script.map(|script| script.ownership),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn aggregate_loader_routes_semantic_spawn_properties_without_raw_shared_bits() {
        let directory = std::env::temp_dir().join(format!(
            "lm-native-spawn-loader-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let spawn = directory.join("Spawn settings.txt");
        fs::write(&spawn, "LMSPAWN1\nsettings 3 true\n").unwrap();
        let expanded = directory.join("Expanded settings.txt");
        fs::write(
            &expanded,
            "LMXSETED1\nlayer3-tilemap true abc 2 3\nsuper-gfx true 1 2 3 4 5 6 101 202 303 404\nlayer3-mode 89abcdef\n",
        )
        .unwrap();
        let features = directory.join("Animation features.txt");
        fs::write(&features, "LMEXFT1\nfeatures true false true false\n").unwrap();
        let layer2 = directory.join("Layer 2 objects.txt");
        fs::write(&layer2, "LML2OBJ1\nobject remove 0\n").unwrap();
        let spec = directory.join("Aggregate.lmnat");
        fs::write(
            &spec,
            "LMNATED1\nlayer2-objects=Layer 2 objects.txt\nexanimation-features=Animation features.txt\nexpanded-settings=Expanded settings.txt\nsprite-spawn=Spawn settings.txt\n",
        )
        .unwrap();

        let loaded = load(&spec).unwrap();
        assert!(matches!(
            loaded.edits.as_slice(),
            [
                NativeLevelAssetsControllerEdit::Layer2Objects(layer2),
                NativeLevelAssetsControllerEdit::ExAnimationFeatureStates {
                    palette: true,
                    vanilla: false,
                    global: true,
                    level: false,
                },
                NativeLevelAssetsControllerEdit::Layer3TilemapSettings {
                    enabled: true,
                    descriptor,
                },
                NativeLevelAssetsControllerEdit::SuperGraphicsBypass(bypass),
                NativeLevelAssetsControllerEdit::Layer3ExpandedMode(flags),
                NativeLevelAssetsControllerEdit::SpriteSpawnProperties {
                    vertical_range: 3,
                    smart_spawn: true,
                },
            ] if layer2 == &[lm_level::ObjectEdit::Remove { index: 0 }]
                && descriptor.packed() == 0xeabc
                && bypass.foreground_background == [1, 2, 3, 4, 5, 6]
                && bypass.sprites == [0x101, 0x202, 0x303, 0x404]
                && flags.packed() == 0x89ab_cdef
        ));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aggregate_loader_routes_tilemap_words_and_native_remaps_in_order() {
        let directory = std::env::temp_dir().join(format!(
            "lm-native-assets-tilemap-loader-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("tiles.txt"),
            "LML2TIL1\nword 1 beef\nremap -1 1,2 8EEF,8EF0\n",
        )
        .unwrap();
        let spec = directory.join("aggregate.lmnat");
        fs::write(&spec, "LMNATED1\nlayer2-tilemap=tiles.txt\n").unwrap();

        assert_eq!(
            load(&spec).unwrap().edits,
            vec![
                NativeLevelAssetsControllerEdit::Layer2TilemapWords(vec![(1, 0xbeef)]),
                NativeLevelAssetsControllerEdit::Layer2TilemapRemap {
                    script: "8EEF,8EF0".into(),
                    global_offset: -1,
                    selection: Some(vec![1, 2]),
                },
            ]
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aggregate_loader_keeps_map16_edits_in_the_cross_domain_plan() {
        let directory = std::env::temp_dir().join(format!(
            "lm-native-assets-map16-loader-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("blocks.txt"),
            "LMM16ED1\nsubtile 01 02 br 1234 10000\n",
        )
        .unwrap();
        let spec = directory.join("aggregate.lmnat");
        fs::write(&spec, "LMNATED1\nlevel=level.txt\nmap16=blocks.txt\n").unwrap();
        fs::write(
            directory.join("level.txt"),
            "LMLEDIT1\nheader last-screen 1f\n",
        )
        .unwrap();

        let loaded = load(&spec).unwrap();
        assert_eq!(loaded.edits.len(), 1);
        assert!(matches!(
            loaded.map16_edits.as_slice(),
            [Map16ControllerEdit::SetSubtile { address, .. }]
                if address.page == 1 && address.tile == 2
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}
