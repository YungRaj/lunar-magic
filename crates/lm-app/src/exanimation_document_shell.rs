use crate::{
    editor_shell::read_bounded_utf8, exanimation_document_spec, exanimation_edit_script,
    file_persistence, read_bounded_bytes, shell_command, spec_text,
};
use lm_app::ExAnimationDocumentController;
use lm_graphics::CompactExAnimationFile;
use std::path::Path;

pub(crate) fn execute_exanimation_document_command(
    session: &mut Option<ExAnimationDocumentController>,
    command: shell_command::ExAnimationDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::ExAnimationDocumentCommand as DocumentCommand;
    match command {
        DocumentCommand::Open(path) => open(session, &path),
        DocumentCommand::Edit(path) => edit(session, &path),
        DocumentCommand::Undo => navigate_history(session, true),
        DocumentCommand::Redo => navigate_history(session, false),
        DocumentCommand::Status => {
            status(session.as_ref());
            Ok(())
        }
        DocumentCommand::Save => save(session),
        DocumentCommand::Close => close(session, false),
        DocumentCommand::Discard => close(session, true),
    }
}

fn navigate_history(
    session: &mut Option<ExAnimationDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no ExAnimation document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "ExAnimation {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<ExAnimationDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("an ExAnimation document is already open".into());
    }
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "ExAnimation document open specification",
    )?;
    let spec = exanimation_document_spec::parse_exanimation_document_open_spec(&text, spec_path)?;
    let raw_modes = read_bounded_bytes(&spec.size_modes, 256, "ExAnimation size-mode table")?;
    if raw_modes.len() != 256 {
        return Err(format!(
            "ExAnimation size-mode table must contain exactly 256 bytes, got {}",
            raw_modes.len()
        )
        .into());
    }
    let modes = raw_modes
        .into_iter()
        .map(|value| value != 0)
        .collect::<Vec<_>>();
    let bytes = read_bounded_bytes(
        &spec.animation,
        CompactExAnimationFile::MAX_FILE_LEN,
        "ExAnimation document",
    )?;
    *session = Some(ExAnimationDocumentController::decode(
        spec.animation,
        &bytes,
        spec.maximum_records,
        &modes,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<ExAnimationDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        exanimation_edit_script::MAX_SCRIPT_LEN,
        "ExAnimation edit script",
    )?;
    let edits = exanimation_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no ExAnimation document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&ExAnimationDocumentController>) {
    if let Some(controller) = session {
        println!(
            "ExAnimation slot {}: {} records, setting {:02X} — revision {} — {} — undo {} — redo {}",
            controller.value().source_slot,
            controller.value().animation.records.len(),
            controller.value().animation.setting,
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
        println!("no ExAnimation document open");
    }
}

fn save(
    session: &mut Option<ExAnimationDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no ExAnimation document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("ExAnimation document saved");
    Ok(())
}

fn close(
    session: &mut Option<ExAnimationDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no ExAnimation document is open")?;
    if controller.is_modified() && !discard {
        return Err("ExAnimation document has unsaved changes; use ex-save or ex-discard".into());
    }
    *session = None;
    println!("ExAnimation document closed");
    Ok(())
}
