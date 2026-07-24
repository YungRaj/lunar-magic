use crate::{
    custom_object_edit_script, editor_shell::read_bounded_utf8, file_persistence,
    read_bounded_bytes, shell_command::ShellCommand,
};
use lm_app::CustomObjectLibraryController;
use std::path::{Path, PathBuf};

pub(crate) fn open_custom_objects(
    session: &mut Option<CustomObjectLibraryController>,
    data_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a custom-object library is already open".into());
    }
    let descriptions_path = custom_object_description_path(data_path)?;
    let controller = CustomObjectLibraryController::decode(
        data_path.to_path_buf(),
        descriptions_path.clone(),
        &read_bounded_bytes(data_path, 0x8000, "custom-object data")?,
        &read_bounded_bytes(&descriptions_path, 0x8000, "custom-object descriptions")?,
    )?;
    println!(
        "custom objects: {} entries — {}",
        controller.library().entries().len(),
        data_path.display()
    );
    *session = Some(controller);
    Ok(())
}

pub(crate) fn execute_custom_object_command(
    session: &mut Option<CustomObjectLibraryController>,
    command: ShellCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ShellCommand::CustomObjectOpen(path) => open_custom_objects(session, &path),
        ShellCommand::CustomObjectEdit(path) => edit_custom_objects(session, &path),
        ShellCommand::CustomObjectUndo => navigate_custom_object_history(session, true),
        ShellCommand::CustomObjectRedo => navigate_custom_object_history(session, false),
        ShellCommand::CustomObjectStatus => {
            show_custom_object_status(session.as_ref());
            Ok(())
        }
        ShellCommand::CustomObjectSave => save_custom_objects(session),
        ShellCommand::CustomObjectClose => close_custom_objects(session, false),
        ShellCommand::CustomObjectDiscard => close_custom_objects(session, true),
        _ => Err("internal non-custom command dispatch".into()),
    }
}

pub(crate) fn navigate_custom_object_history(
    session: &mut Option<CustomObjectLibraryController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no custom-object library is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "custom-object {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    show_custom_object_status(session.as_ref());
    Ok(())
}

fn custom_object_description_path(data_path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let name = data_path
        .file_name()
        .ok_or("custom-object data path has no file name")?;
    let mut description_name = name.to_os_string();
    description_name.push("t");
    Ok(data_path.with_file_name(description_name))
}

pub(crate) fn edit_custom_objects(
    session: &mut Option<CustomObjectLibraryController>,
    script_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        script_path,
        custom_object_edit_script::MAX_SCRIPT_LEN,
        "custom-object edit",
    )?;
    let edits = custom_object_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no custom-object library is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    show_custom_object_status(session.as_ref());
    Ok(())
}

fn show_custom_object_status(session: Option<&CustomObjectLibraryController>) {
    if let Some(controller) = session {
        println!(
            "custom objects: {} entries — revision {} — {} — undo {} — redo {}",
            controller.library().entries().len(),
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
        println!("no custom-object library open");
    }
}

pub(crate) fn save_custom_objects(
    session: &mut Option<CustomObjectLibraryController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no custom-object library is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing_pair(
        (&snapshot.data_path, &snapshot.data),
        (&snapshot.descriptions_path, &snapshot.descriptions),
    ) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("custom-object sidecars saved");
    Ok(())
}

pub(crate) fn close_custom_objects(
    session: &mut Option<CustomObjectLibraryController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no custom-object library is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "custom-object library has unsaved changes; use custom-save or custom-discard".into(),
        );
    }
    *session = None;
    println!("custom-object library closed");
    Ok(())
}
