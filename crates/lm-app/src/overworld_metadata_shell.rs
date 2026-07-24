use crate::{
    editor_shell::read_bounded_utf8, file_persistence, overworld_metadata_edit_script,
    read_bounded_bytes, shell_command,
};
use lm_app::OverworldMetadataController;
use std::path::Path;

pub(crate) fn execute_metadata_document_command(
    session: &mut Option<OverworldMetadataController>,
    command: shell_command::MetadataDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::MetadataDocumentCommand as DocumentCommand;
    match command {
        DocumentCommand::Open(path) => open_metadata_document(session, &path),
        DocumentCommand::Edit(path) => edit_metadata_document(session, &path),
        DocumentCommand::Undo => navigate_metadata_history(session, true),
        DocumentCommand::Redo => navigate_metadata_history(session, false),
        DocumentCommand::Status => {
            show_metadata_document_status(session.as_ref());
            Ok(())
        }
        DocumentCommand::Save => save_metadata_document(session),
        DocumentCommand::Close => close_metadata_document(session, false),
        DocumentCommand::Discard => close_metadata_document(session, true),
    }
}

pub(crate) fn navigate_metadata_history(
    session: &mut Option<OverworldMetadataController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no overworld metadata document is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "overworld metadata {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    show_metadata_document_status(session.as_ref());
    Ok(())
}

pub(crate) fn open_metadata_document(
    session: &mut Option<OverworldMetadataController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("an overworld metadata document is already open".into());
    }
    let controller = OverworldMetadataController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(
            path,
            lm_overworld::OverworldMetadata::MAX_FILE_LEN,
            "metadata",
        )?,
    )?;
    *session = Some(controller);
    show_metadata_document_status(session.as_ref());
    Ok(())
}

pub(crate) fn edit_metadata_document(
    session: &mut Option<OverworldMetadataController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        overworld_metadata_edit_script::MAX_SCRIPT_LEN,
        "overworld metadata edit",
    )?;
    let edits = overworld_metadata_edit_script::parse(&text)?;
    let controller = session
        .as_mut()
        .ok_or("no overworld metadata document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    show_metadata_document_status(session.as_ref());
    Ok(())
}

fn show_metadata_document_status(session: Option<&OverworldMetadataController>) {
    if let Some(controller) = session {
        let metadata = controller.metadata();
        println!(
            "overworld metadata: {} names, {} starts, {} settings — revision {} — {} — undo {} — redo {}",
            metadata.level_names.len(),
            metadata.player_starts.len(),
            metadata.submap_settings.len(),
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
        println!("no overworld metadata document open");
    }
}

pub(crate) fn save_metadata_document(
    session: &mut Option<OverworldMetadataController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no overworld metadata document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("overworld metadata saved");
    Ok(())
}

pub(crate) fn close_metadata_document(
    session: &mut Option<OverworldMetadataController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no overworld metadata document is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "overworld metadata has unsaved changes; use metadata-save or metadata-discard".into(),
        );
    }
    *session = None;
    println!("overworld metadata document closed");
    Ok(())
}
