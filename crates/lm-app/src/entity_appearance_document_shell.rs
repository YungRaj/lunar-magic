use crate::{
    editor_shell::read_bounded_utf8, entity_appearance_edit_script, file_persistence,
    read_bounded_bytes, shell_command,
};
use lm_app::EntityAppearanceDocumentController;
use lm_level::EntityAppearanceFile;
use std::path::Path;

pub(crate) fn execute_entity_appearance_document_command(
    session: &mut Option<EntityAppearanceDocumentController>,
    command: shell_command::EntityAppearanceDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::EntityAppearanceDocumentCommand as Command;
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
    session: &mut Option<EntityAppearanceDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no entity appearance document is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "entity appearance {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<EntityAppearanceDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("an entity appearance document is already open".into());
    }
    let maximum = EntityAppearanceFile::HEADER_LEN
        + EntityAppearanceFile::MAX_APPEARANCES * EntityAppearanceFile::RECORD_LEN;
    *session = Some(EntityAppearanceDocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(path, maximum, "entity appearance document")?,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<EntityAppearanceDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        entity_appearance_edit_script::MAX_SCRIPT_LEN,
        "entity appearance edit",
    )?;
    let edits = entity_appearance_edit_script::parse(&text)?;
    let controller = session
        .as_mut()
        .ok_or("no entity appearance document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&EntityAppearanceDocumentController>) {
    if let Some(controller) = session {
        println!(
            "entity appearances: {} records — revision {} — {} — undo {} — redo {}",
            controller.value().appearances.len(),
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
        println!("no entity appearance document open");
    }
}

fn save(
    session: &mut Option<EntityAppearanceDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no entity appearance document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("entity appearance document saved");
    Ok(())
}

fn close(
    session: &mut Option<EntityAppearanceDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no entity appearance document is open")?;
    if controller.is_modified() && !discard {
        return Err("entity appearance document has unsaved changes; use entity-app-save or entity-app-discard".into());
    }
    *session = None;
    println!("entity appearance document closed");
    Ok(())
}

#[cfg(test)]
#[path = "entity_appearance_document_shell_tests.rs"]
mod tests;
