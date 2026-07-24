use crate::{
    editor_shell::read_bounded_utf8, file_persistence, overworld_appearance_edit_script,
    read_bounded_bytes, shell_command,
};
use lm_app::OverworldAppearanceDocumentController;
use lm_overworld::SpriteAppearanceFile;
use std::path::Path;

pub(crate) fn execute_overworld_appearance_document_command(
    session: &mut Option<OverworldAppearanceDocumentController>,
    command: shell_command::OverworldAppearanceDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::OverworldAppearanceDocumentCommand as Command;
    match command {
        Command::Open(path) => open(session, &path),
        Command::Edit(path) => edit(session, &path),
        Command::Undo => navigate_history(session, true),
        Command::Redo => navigate_history(session, false),
        Command::Status => {
            status(session.as_ref());
            Ok(())
        }
        Command::Save => save(session),
        Command::Close => close(session, false),
        Command::Discard => close(session, true),
    }
}

fn navigate_history(
    session: &mut Option<OverworldAppearanceDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no overworld appearance document is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "overworld appearance {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<OverworldAppearanceDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("an overworld appearance document is already open".into());
    }
    let maximum = SpriteAppearanceFile::HEADER_LEN
        + SpriteAppearanceFile::MAX_DEFINITIONS * 12
        + SpriteAppearanceFile::MAX_PARTS * 12;
    *session = Some(OverworldAppearanceDocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(path, maximum, "overworld appearance document")?,
    )?);
    status(session.as_ref());
    Ok(())
}
fn edit(
    session: &mut Option<OverworldAppearanceDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        overworld_appearance_edit_script::MAX_SCRIPT_LEN,
        "overworld appearance edit",
    )?;
    let edits = overworld_appearance_edit_script::parse(&text)?;
    let controller = session
        .as_mut()
        .ok_or("no overworld appearance document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}
fn status(session: Option<&OverworldAppearanceDocumentController>) {
    if let Some(controller) = session {
        let parts: usize = controller
            .value()
            .definitions
            .iter()
            .map(|value| value.parts.len())
            .sum();
        println!(
            "overworld appearances: {} definitions, {parts} parts — revision {} — {} — undo {} — redo {}",
            controller.value().definitions.len(),
            controller.revision(),
            if controller.is_modified() {
                "modified"
            } else {
                "saved"
            },
            if controller.can_undo() { "yes" } else { "no" },
            if controller.can_redo() { "yes" } else { "no" }
        );
    } else {
        println!("no overworld appearance document open");
    }
}
fn save(
    session: &mut Option<OverworldAppearanceDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no overworld appearance document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("overworld appearance document saved");
    Ok(())
}
fn close(
    session: &mut Option<OverworldAppearanceDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no overworld appearance document is open")?;
    if controller.is_modified() && !discard {
        return Err("overworld appearance document has unsaved changes; use world-app-save or world-app-discard".into());
    }
    *session = None;
    println!("overworld appearance document closed");
    Ok(())
}

#[cfg(test)]
#[path = "overworld_appearance_document_shell_tests.rs"]
mod tests;
