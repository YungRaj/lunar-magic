//! Shared loading of bounded aggregate composition scripts.

use crate::{
    editor_shell::read_bounded_utf8, exanimation_edit_script, expanded_settings_edit_script,
    level_edit_script, native_assets_edit_spec, palette_edit_script,
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
    Ok(LoadedNativeAssetsEdits {
        edits,
        palette_ownership: palette_script.map(|script| script.ownership),
    })
}
