use crate::{
    editor_shell::read_bounded_utf8, file_persistence, overworld_path_edit_script,
    read_bounded_bytes, shell_command,
};
use lm_app::OverworldPathController;
use std::path::Path;

pub(crate) fn execute_path_document_command(
    session: &mut Option<OverworldPathController>,
    command: shell_command::PathDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::PathDocumentCommand as DocumentCommand;
    match command {
        DocumentCommand::Open(path) => open_path_document(session, &path),
        DocumentCommand::Edit(path) => edit_path_document(session, &path),
        DocumentCommand::Undo => navigate_path_history(session, true),
        DocumentCommand::Redo => navigate_path_history(session, false),
        DocumentCommand::Status => {
            show_path_document_status(session.as_ref());
            Ok(())
        }
        DocumentCommand::Save => save_path_document(session),
        DocumentCommand::Close => close_path_document(session, false),
        DocumentCommand::Discard => close_path_document(session, true),
    }
}

pub(crate) fn navigate_path_history(
    session: &mut Option<OverworldPathController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no overworld path document is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "overworld path {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    show_path_document_status(session.as_ref());
    Ok(())
}

pub(crate) fn open_path_document(
    session: &mut Option<OverworldPathController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("an overworld path document is already open".into());
    }
    let controller = OverworldPathController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(
            path,
            lm_overworld::OverworldPathGraph::MAX_FILE_LEN,
            "overworld path",
        )?,
        true,
    )?;
    *session = Some(controller);
    show_path_document_status(session.as_ref());
    Ok(())
}

pub(crate) fn edit_path_document(
    session: &mut Option<OverworldPathController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        overworld_path_edit_script::MAX_SCRIPT_LEN,
        "overworld path edit",
    )?;
    let edits = overworld_path_edit_script::parse(&text)?;
    let controller = session
        .as_mut()
        .ok_or("no overworld path document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    show_path_document_status(session.as_ref());
    Ok(())
}

fn show_path_document_status(session: Option<&OverworldPathController>) {
    if let Some(controller) = session {
        println!(
            "overworld paths: {} nodes, {} edges — revision {} — {} — undo {} — redo {}",
            controller.graph().nodes.len(),
            controller.graph().edges.len(),
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
        println!("no overworld path document open");
    }
}

pub(crate) fn save_path_document(
    session: &mut Option<OverworldPathController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no overworld path document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("overworld paths saved");
    Ok(())
}

pub(crate) fn close_path_document(
    session: &mut Option<OverworldPathController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no overworld path document is open")?;
    if controller.is_modified() && !discard {
        return Err("overworld paths have unsaved changes; use path-save or path-discard".into());
    }
    *session = None;
    println!("overworld path document closed");
    Ok(())
}
