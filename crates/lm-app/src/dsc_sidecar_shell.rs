use crate::{file_persistence, read_bounded_bytes, shell_command::ShellCommand};
use lm_app::DscSidecarController;
use lm_level::MAX_DSC_SOURCE_LEN;
use std::path::Path;

pub(crate) fn execute(
    session: &mut Option<DscSidecarController>,
    command: ShellCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ShellCommand::DscSidecarOpen(path) => open(session, &path),
        ShellCommand::DscSidecarReplace(path) => replace(session, &path),
        ShellCommand::DscSidecarUndo => navigate_history(session, true),
        ShellCommand::DscSidecarRedo => navigate_history(session, false),
        ShellCommand::DscSidecarStatus => {
            status(session.as_ref());
            Ok(())
        }
        ShellCommand::DscSidecarSave => save(session),
        ShellCommand::DscSidecarClose => close(session, false),
        ShellCommand::DscSidecarDiscard => close(session, true),
        _ => Err("internal non-DSC command dispatch".into()),
    }
}
fn navigate_history(
    session: &mut Option<DscSidecarController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no DSC sidecar document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "DSC {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<DscSidecarController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a DSC sidecar document is already open".into());
    }
    let bytes = read_bounded_bytes(path, MAX_DSC_SOURCE_LEN, "DSC sidecar")?;
    *session = Some(DscSidecarController::decode(path.to_path_buf(), &bytes)?);
    status(session.as_ref());
    Ok(())
}

fn replace(
    session: &mut Option<DscSidecarController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = read_bounded_bytes(path, MAX_DSC_SOURCE_LEN, "replacement DSC sidecar")?;
    let controller = session.as_mut().ok_or("no DSC sidecar document is open")?;
    controller.replace_source(controller.revision(), &bytes)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&DscSidecarController>) {
    if let Some(controller) = session {
        println!(
            "DSC sidecar: {} parsed entries — revision {} — {} — undo {} — redo {}",
            controller.value().entries().len(),
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
        println!("no DSC sidecar document open");
    }
}

fn save(session: &mut Option<DscSidecarController>) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no DSC sidecar document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("DSC sidecar saved");
    Ok(())
}

fn close(
    session: &mut Option<DscSidecarController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no DSC sidecar document is open")?;
    if controller.is_modified() && !discard {
        return Err("DSC sidecar has unsaved changes; use dsc-save or dsc-discard".into());
    }
    *session = None;
    println!("DSC sidecar closed");
    Ok(())
}
