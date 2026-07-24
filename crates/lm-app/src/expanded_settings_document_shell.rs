use crate::{
    editor_shell::read_bounded_utf8, expanded_settings_edit_script, file_persistence,
    read_bounded_bytes, shell_command,
};
use lm_app::ExpandedSettingsDocumentController;
use lm_level::ExpandedLevelSettingsRecord;
use std::path::Path;

pub(crate) fn execute(
    session: &mut Option<ExpandedSettingsDocumentController>,
    command: shell_command::ExpandedSettingsDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::ExpandedSettingsDocumentCommand as Command;
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
    session: &mut Option<ExpandedSettingsDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no expanded-settings document is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "expanded-settings {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<ExpandedSettingsDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("an expanded-settings document is already open".into());
    }
    *session = Some(ExpandedSettingsDocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(
            path,
            ExpandedLevelSettingsRecord::ENCODED_LEN,
            "expanded settings",
        )?,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<ExpandedSettingsDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let script = read_bounded_utf8(
        path,
        expanded_settings_edit_script::MAX_SCRIPT_LEN,
        "expanded-settings edit",
    )?;
    let edits = expanded_settings_edit_script::parse(&script)?;
    let controller = session
        .as_mut()
        .ok_or("no expanded-settings document is open")?;
    controller.apply_word_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&ExpandedSettingsDocumentController>) {
    if let Some(controller) = session {
        println!(
            "expanded settings: 16 native words — revision {} — {} — undo {} — redo {}",
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
        println!("no expanded-settings document open");
    }
}

fn save(
    session: &mut Option<ExpandedSettingsDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no expanded-settings document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("expanded-settings document saved");
    Ok(())
}

fn close(
    session: &mut Option<ExpandedSettingsDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no expanded-settings document is open")?;
    if controller.is_modified() && !discard {
        return Err("expanded-settings document has unsaved changes; use expanded-settings-save or expanded-settings-discard".into());
    }
    *session = None;
    println!("expanded-settings document closed");
    Ok(())
}
