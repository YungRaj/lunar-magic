use crate::{
    editor_shell::read_bounded_utf8, file_persistence, level_edit_script,
    native_level_document_spec, read_bounded_bytes, shell_command, spec_text,
};
use lm_app::NativeLevelDocumentController;
use lm_level::{NativeLevelFile, SpriteLengthTable};
use std::path::Path;

pub(crate) fn execute_native_level_document_command(
    session: &mut Option<NativeLevelDocumentController>,
    command: shell_command::NativeLevelDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::NativeLevelDocumentCommand as DocumentCommand;
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
    session: &mut Option<NativeLevelDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no native-level document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "native-level {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<NativeLevelDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a native-level document is already open".into());
    }
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "native-level document open specification",
    )?;
    let spec = native_level_document_spec::parse(&text, spec_path)?;
    let sprite_lengths = match spec.sprite_lengths {
        native_level_document_spec::SpriteLengthSource::Standard => SpriteLengthTable::standard(),
        native_level_document_spec::SpriteLengthSource::File(path) => {
            let bytes =
                read_bounded_bytes(&path, SpriteLengthTable::ENCODED_LEN, "sprite-length table")?;
            SpriteLengthTable::decode(&bytes).map_err(|actual| {
                format!(
                    "sprite-length table must contain exactly {} bytes, got {actual}",
                    SpriteLengthTable::ENCODED_LEN
                )
            })?
        }
    };
    let maximum = NativeLevelFile::HEADER_LEN + 2 * NativeLevelFile::MAX_STREAM_LEN;
    let bytes = read_bounded_bytes(&spec.level, maximum, "native-level document")?;
    *session = Some(NativeLevelDocumentController::decode(
        spec.level,
        &bytes,
        sprite_lengths,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<NativeLevelDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, level_edit_script::MAX_SCRIPT_LEN, "native-level edit")?;
    let edits = level_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no native-level document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&NativeLevelDocumentController>) {
    if let Some(controller) = session {
        println!(
            "native level {:03X}: {} objects, {} sprite tokens — revision {} — {} — undo {} — redo {}",
            controller.value().source_level,
            controller.value().layer1.objects.records.len(),
            controller.value().sprites.tokens.len(),
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
        println!("no native-level document open");
    }
}

fn save(
    session: &mut Option<NativeLevelDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no native-level document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("native-level document saved");
    Ok(())
}

fn close(
    session: &mut Option<NativeLevelDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no native-level document is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "native-level document has unsaved changes; use native-level-save or native-level-discard"
                .into(),
        );
    }
    *session = None;
    println!("native-level document closed");
    Ok(())
}

#[cfg(test)]
#[path = "native_level_document_shell_tests.rs"]
mod tests;
