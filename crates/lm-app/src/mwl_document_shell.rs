use crate::{
    editor_shell::read_bounded_utf8, file_persistence, mwl_edit_script, mwl_layer3_settings_spec,
    mwl_optional_assets_edit_spec, mwl_optional_assets_spec, read_bounded_bytes, shell_command,
    spec_text,
};
use lm_app::MwlDocumentController;
use lm_level::{MwlFile, MwlLevelHeaderSection, MwlSectionKind};
use lm_project::{MAX_MWL_OPTIONAL_ASSETS_EDIT_SCRIPT_LEN, parse_mwl_optional_assets_edit_script};
use std::path::Path;

pub(crate) fn execute_mwl_document_command(
    session: &mut Option<MwlDocumentController>,
    command: shell_command::MwlDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::MwlDocumentCommand as DocumentCommand;
    match command {
        DocumentCommand::Open(path) => open_mwl_document(session, &path),
        DocumentCommand::Edit(path) => edit_mwl_document(session, &path),
        DocumentCommand::ImportOptionalAssets(path) => import_optional_assets(session, &path),
        DocumentCommand::EditOptionalAssets(path) => edit_optional_assets(session, &path),
        DocumentCommand::EditLayer3Settings(path) => edit_layer3_settings(session, &path),
        DocumentCommand::Undo => navigate_mwl_document_history(session, true),
        DocumentCommand::Redo => navigate_mwl_document_history(session, false),
        DocumentCommand::Status => {
            show_mwl_document_status(session.as_ref());
            Ok(())
        }
        DocumentCommand::Save => save_mwl_document(session),
        DocumentCommand::Close => close_mwl_document(session, false),
        DocumentCommand::Discard => close_mwl_document(session, true),
    }
}

fn edit_layer3_settings(
    session: &mut Option<MwlDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "MWL Layer 3 settings specification",
    )?;
    let spec = mwl_layer3_settings_spec::parse(&text)?;
    let controller = session.as_mut().ok_or("no MWL document is open")?;
    controller.apply_layer3_settings(
        controller.revision(),
        spec.enabled,
        spec.descriptor,
        spec.expanded_mode,
    )?;
    show_mwl_document_status(session.as_ref());
    Ok(())
}

fn edit_optional_assets(
    session: &mut Option<MwlDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "MWL optional-assets edit specification",
    )?;
    let spec = mwl_optional_assets_edit_spec::parse(&text, spec_path)?;
    let modes = exact_modes(&spec.size_modes)?;
    let edit_text = read_bounded_utf8(
        &spec.edits,
        MAX_MWL_OPTIONAL_ASSETS_EDIT_SCRIPT_LEN,
        "MWL optional-assets edit script",
    )?;
    let edits = parse_mwl_optional_assets_edit_script(&edit_text)?;
    let controller = session.as_mut().ok_or("no MWL document is open")?;
    controller.apply_optional_assets_edits(
        controller.revision(),
        spec.maximum_records,
        &modes,
        &edits,
    )?;
    show_mwl_document_status(session.as_ref());
    Ok(())
}

fn import_optional_assets(
    session: &mut Option<MwlDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "MWL optional-assets import specification",
    )?;
    let spec = mwl_optional_assets_spec::parse(&text, spec_path)?;
    let modes = exact_modes(&spec.size_modes)?;
    let source = MwlFile::decode(&read_bounded_bytes(
        &spec.source,
        MwlFile::MAX_FILE_BYTES,
        "source MWL",
    )?)?;
    let controller = session.as_mut().ok_or("no MWL document is open")?;
    controller.import_optional_assets(
        controller.revision(),
        &source,
        spec.maximum_records,
        &modes,
    )?;
    show_mwl_document_status(session.as_ref());
    Ok(())
}

fn exact_modes(path: &Path) -> Result<[bool; 256], Box<dyn std::error::Error>> {
    let raw_modes = read_bounded_bytes(path, 256, "ExAnimation size-mode table")?;
    if raw_modes.len() != 256 {
        return Err(format!(
            "ExAnimation size-mode table must contain exactly 256 bytes, got {}",
            raw_modes.len()
        )
        .into());
    }
    Ok(std::array::from_fn(|index| raw_modes[index] != 0))
}

fn navigate_mwl_document_history(
    session: &mut Option<MwlDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no MWL document is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "MWL {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    show_mwl_document_status(session.as_ref());
    Ok(())
}

fn open_mwl_document(
    session: &mut Option<MwlDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("an MWL document is already open".into());
    }
    let controller = MwlDocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(path, MwlFile::MAX_FILE_BYTES, "MWL")?,
    )?;
    *session = Some(controller);
    show_mwl_document_status(session.as_ref());
    Ok(())
}

fn edit_mwl_document(
    session: &mut Option<MwlDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(path, mwl_edit_script::MAX_SCRIPT_LEN, "MWL edit")?;
    let edits = mwl_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no MWL document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    show_mwl_document_status(session.as_ref());
    Ok(())
}

fn show_mwl_document_status(session: Option<&MwlDocumentController>) {
    if let Some(controller) = session {
        let file = controller.value();
        let level = MwlLevelHeaderSection::decode(
            &file.sections[MwlSectionKind::LevelHeader as usize].bytes,
        )
        .map_or_else(
            |_| "opaque".to_owned(),
            |header| format!("{:03x}", header.level_number()),
        );
        let populated = file
            .sections
            .iter()
            .filter(|section| !section.bytes.is_empty())
            .count();
        println!(
            "MWL level {level}: {populated}/{} sections — revision {} — {} — undo {} — redo {}",
            MwlFile::SECTION_COUNT,
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
        println!("no MWL document open");
    }
}

fn save_mwl_document(
    session: &mut Option<MwlDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no MWL document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("MWL document saved");
    Ok(())
}

fn close_mwl_document(
    session: &mut Option<MwlDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no MWL document is open")?;
    if controller.is_modified() && !discard {
        return Err("MWL document has unsaved changes; use mwl-save or mwl-discard".into());
    }
    *session = None;
    println!("MWL document closed");
    Ok(())
}

#[cfg(test)]
#[path = "mwl_document_shell_tests.rs"]
mod tests;
