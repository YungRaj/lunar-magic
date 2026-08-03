//! Shared loading of bounded aggregate composition scripts.

use crate::{
    editor_shell::read_bounded_utf8, exanimation_edit_script, expanded_settings_edit_script,
    level_edit_script, native_assets_edit_spec, palette_edit_script, sprite_spawn_edit_script,
};
use lm_app::NativeLevelAssetsControllerEdit;
use lm_graphics::PaletteOwnership;
use std::path::Path;

pub(crate) struct LoadedNativeAssetsEdits {
    pub edits: Vec<NativeLevelAssetsControllerEdit>,
    pub palette_ownership: Option<PaletteOwnership>,
}

pub(crate) fn load(path: &Path) -> Result<LoadedNativeAssetsEdits, Box<dyn std::error::Error>> {
    let spec = native_assets_edit_spec::read(path)?;
    let mut edits = Vec::new();
    if let Some(path) = spec.level {
        let text = read_bounded_utf8(&path, level_edit_script::MAX_SCRIPT_LEN, "level edit")?;
        edits.push(NativeLevelAssetsControllerEdit::Level(
            level_edit_script::parse(&text)?,
        ));
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
    if let Some(path) = spec.expanded_settings {
        let text = read_bounded_utf8(
            &path,
            expanded_settings_edit_script::MAX_SCRIPT_LEN,
            "expanded-settings edit",
        )?;
        edits.push(NativeLevelAssetsControllerEdit::ExpandedSettingsWords(
            expanded_settings_edit_script::parse(&text)?,
        ));
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
        fs::write(&spawn, "LMSPAWN1\nsettings 3 true\nboundary-air false\n").unwrap();
        let spec = directory.join("Aggregate.lmnat");
        fs::write(&spec, "LMNATED1\nsprite-spawn=Spawn settings.txt\n").unwrap();

        let loaded = load(&spec).unwrap();
        assert!(matches!(
            loaded.edits.as_slice(),
            [
                NativeLevelAssetsControllerEdit::SpriteSpawnProperties {
                    vertical_range: 3,
                    smart_spawn: true,
                },
                NativeLevelAssetsControllerEdit::SpriteBoundaryInteractionAir(false),
            ]
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}
