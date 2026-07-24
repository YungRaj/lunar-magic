use crate::{
    editor_shell::read_bounded_utf8, file_persistence, layer3_edit_script, read_bounded_bytes,
    shell_command,
};
use lm_app::Layer3DocumentController;
use std::path::Path;

pub(crate) fn execute_layer3_document_command(
    session: &mut Option<Layer3DocumentController>,
    command: shell_command::Layer3DocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::Layer3DocumentCommand as DocumentCommand;
    match command {
        DocumentCommand::Open(path) => open_layer3_document(session, &path),
        DocumentCommand::Edit(path) => edit_layer3_document(session, &path),
        DocumentCommand::Undo => navigate_layer3_document_history(session, true),
        DocumentCommand::Redo => navigate_layer3_document_history(session, false),
        DocumentCommand::Status => {
            show_layer3_document_status(session.as_ref());
            Ok(())
        }
        DocumentCommand::Save => save_layer3_document(session),
        DocumentCommand::Close => close_layer3_document(session, false),
        DocumentCommand::Discard => close_layer3_document(session, true),
    }
}

pub(crate) fn navigate_layer3_document_history(
    session: &mut Option<Layer3DocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no Layer 3 document is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "Layer 3 {} {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    show_layer3_document_status(session.as_ref());
    Ok(())
}

pub(crate) fn open_layer3_document(
    session: &mut Option<Layer3DocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a Layer 3 document is already open".into());
    }
    let controller = Layer3DocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(path, lm_level::Layer3File::MAX_ENCODED_LEN, "Layer 3")?,
    )?;
    *session = Some(controller);
    show_layer3_document_status(session.as_ref());
    Ok(())
}

pub(crate) fn edit_layer3_document(
    session: &mut Option<Layer3DocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, layer3_edit_script::MAX_SCRIPT_LEN, "Layer 3 edit")?;
    let edits = layer3_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no Layer 3 document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    show_layer3_document_status(session.as_ref());
    Ok(())
}

fn show_layer3_document_status(session: Option<&Layer3DocumentController>) {
    if let Some(controller) = session {
        println!(
            "Layer 3: {} tilemap bytes, {} remap bytes — revision {} — {} — undo {} — redo {}",
            controller.value().0.tilemap.len(),
            controller.value().0.remap_commands.len(),
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
        println!("no Layer 3 document open");
    }
}

pub(crate) fn save_layer3_document(
    session: &mut Option<Layer3DocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no Layer 3 document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("Layer 3 document saved");
    Ok(())
}

pub(crate) fn close_layer3_document(
    session: &mut Option<Layer3DocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no Layer 3 document is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "Layer 3 document has unsaved changes; use layer3-save or layer3-discard".into(),
        );
    }
    *session = None;
    println!("Layer 3 document closed");
    Ok(())
}
