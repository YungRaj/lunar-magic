use super::read_bounded_utf8;
use crate::expanded_settings_edit_script;
use lm_app::{AppState, RevisionProfileControllers};
use std::path::Path;

pub(crate) fn edit_expanded_settings_word(
    app: &mut AppState,
    index: usize,
    value: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let profiled = app.profiled_controller_snapshot()?;
    let mut controller = profiled
        .profile
        .decode_expanded_settings(&profiled.snapshot)?;
    controller.set_word(index, value)?;
    app.dispatch(
        controller
            .prepare_commit(format!("Edit expanded settings word {index:x}"))?
            .into_command(),
    )?;
    Ok(())
}

pub(crate) fn edit_expanded_settings(
    app: &mut AppState,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        expanded_settings_edit_script::MAX_SCRIPT_LEN,
        "expanded-settings edit",
    )?;
    let script = expanded_settings_edit_script::parse(&text)?;
    let profiled = app.profiled_controller_snapshot()?;
    let mut controller = profiled
        .profile
        .decode_expanded_settings(&profiled.snapshot)?;
    let edits = script.resolve(controller.record())?;
    controller.apply_word_edits(&edits)?;
    app.dispatch(
        controller
            .prepare_commit("Apply expanded-settings edit script")?
            .into_command(),
    )?;
    Ok(())
}
