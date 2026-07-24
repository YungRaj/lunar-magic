use crate::file_persistence;
use lm_app::CompleteLevelDocumentController;

pub(super) fn navigate_complete_level_history(
    session: &mut Option<CompleteLevelDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no complete level document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "complete level {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    show_complete_level_document_status(session.as_ref());
    Ok(())
}

pub(super) fn show_complete_level_document_status(
    session: Option<&CompleteLevelDocumentController>,
) {
    if let Some(controller) = session {
        let level = &controller.value().0;
        println!(
            "Complete level {:03X}: {} entrances, {} screen exits, {} secondary exits, {} Map16 overrides — revision {} — {} — undo {} — redo {}",
            level.number,
            level.entrances.len(),
            level.screen_exits.len(),
            level.secondary_exits.len(),
            level.map16_overrides.len(),
            controller.revision(),
            if controller.is_modified() {
                "modified"
            } else {
                "saved"
            },
            if controller.can_undo() {
                "available"
            } else {
                "unavailable"
            },
            if controller.can_redo() {
                "available"
            } else {
                "unavailable"
            }
        );
    } else {
        println!("no complete level document open");
    }
}

pub(crate) fn save_complete_level_document(
    session: &mut Option<CompleteLevelDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no complete level document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("complete level document saved");
    Ok(())
}

pub(crate) fn close_complete_level_document(
    session: &mut Option<CompleteLevelDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no complete level document is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "complete level document has unsaved changes; use bundle-save or bundle-discard".into(),
        );
    }
    *session = None;
    println!("complete level document closed");
    Ok(())
}
