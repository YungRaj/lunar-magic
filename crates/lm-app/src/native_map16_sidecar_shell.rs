use crate::{
    editor_shell::read_bounded_utf8, file_persistence, native_map16_sidecar_edit_script,
    native_map16_sidecar_spec, read_bounded_bytes, shell_command::ShellCommand, spec_text,
};
use lm_app::{NativeMap16SidecarController, NativeMap16SidecarDocumentKind};
use lm_level::{M16Sidecar, S16Sidecar};
use std::path::Path;

pub(crate) fn execute(
    session: &mut Option<NativeMap16SidecarController>,
    command: ShellCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ShellCommand::NativeMap16SidecarOpen(path) => open(session, &path),
        ShellCommand::NativeMap16SidecarEdit(path) => edit(session, &path),
        ShellCommand::NativeMap16SidecarUndo => navigate_history(session, true),
        ShellCommand::NativeMap16SidecarRedo => navigate_history(session, false),
        ShellCommand::NativeMap16SidecarStatus => {
            status(session.as_ref());
            Ok(())
        }
        ShellCommand::NativeMap16SidecarSave => save(session),
        ShellCommand::NativeMap16SidecarClose => close(session, false),
        ShellCommand::NativeMap16SidecarDiscard => close(session, true),
        _ => Err("internal non-native-Map16-sidecar command dispatch".into()),
    }
}
fn navigate_history(
    session: &mut Option<NativeMap16SidecarController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no native Map16 sidecar document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "native Map16 sidecar {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<NativeMap16SidecarController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a native Map16 sidecar document is already open".into());
    }
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "native Map16 sidecar specification",
    )?;
    let spec = native_map16_sidecar_spec::parse(&text, spec_path)?;
    let maximum = match spec.kind {
        NativeMap16SidecarDocumentKind::M16 => M16Sidecar::ENCODED_LEN,
        NativeMap16SidecarDocumentKind::S16 => S16Sidecar::CAPACITY,
    };
    let bytes = read_bounded_bytes(&spec.file, maximum, "native Map16 sidecar")?;
    *session = Some(NativeMap16SidecarController::decode(
        spec.file, spec.kind, &bytes,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<NativeMap16SidecarController>,
    script: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        script,
        native_map16_sidecar_edit_script::MAX_SCRIPT_LEN,
        "native Map16 sidecar edit",
    )?;
    let edits = native_map16_sidecar_edit_script::parse(&text)?;
    let controller = session
        .as_mut()
        .ok_or("no native Map16 sidecar document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&NativeMap16SidecarController>) {
    if let Some(controller) = session {
        println!(
            "native {:?} sidecar: {} entries — revision {} — {} — undo {} — redo {}",
            controller.value().kind(),
            controller.value().entry_count(),
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
        println!("no native Map16 sidecar document open");
    }
}

fn save(
    session: &mut Option<NativeMap16SidecarController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no native Map16 sidecar document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("native Map16 sidecar saved");
    Ok(())
}

fn close(
    session: &mut Option<NativeMap16SidecarController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no native Map16 sidecar document is open")?;
    if controller.is_modified() && !discard {
        return Err("native Map16 sidecar has unsaved changes; use native-sidecar-save or native-sidecar-discard".into());
    }
    *session = None;
    println!("native Map16 sidecar closed");
    Ok(())
}
