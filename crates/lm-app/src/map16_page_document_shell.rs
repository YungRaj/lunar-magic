use crate::{
    editor_shell::read_bounded_utf8, file_persistence, map16_page_edit_script, map16_render_spec,
    read_bounded_bytes, shell_command, spec_text,
};
use lm_app::Map16PageDocumentController;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::Map16PageFile;
use lm_render::{encode_png, render_portable_map16_page};
use std::path::Path;

pub(crate) fn execute_map16_page_document_command(
    session: &mut Option<Map16PageDocumentController>,
    command: shell_command::Map16PageDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::Map16PageDocumentCommand as DocumentCommand;
    match command {
        DocumentCommand::Open(path) => open(session, &path),
        DocumentCommand::Edit(path) => edit(session, &path),
        DocumentCommand::Render(path) => render(session.as_ref(), &path),
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
    session: &mut Option<Map16PageDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no Map16 page document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "Map16 page {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn render(
    session: Option<&Map16PageDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.ok_or("no Map16 page document is open")?;
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "Map16 page document render specification",
    )?;
    let spec = map16_render_spec::parse_map16_page_document_render_spec(&text, spec_path)?;
    let graphics = GraphicsInterchangeFile::decode(&read_bounded_bytes(
        &spec.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
        "graphics",
    )?)?;
    let palette = PaletteInterchangeFile::decode(&read_bounded_bytes(
        &spec.palette,
        PaletteInterchangeFile::MAX_FILE_LEN,
        "palette",
    )?)?;
    let canvas = crate::viewport_spec::render(
        render_portable_map16_page(&graphics, &palette, controller.value())?,
        spec.viewport,
        spec.overlays.as_deref(),
    )?;
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "open Map16 page rendered: {}x{} — revision {} — {}",
        canvas.width(),
        canvas.height(),
        controller.revision(),
        spec.output.display()
    );
    Ok(())
}

fn open(
    session: &mut Option<Map16PageDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a Map16 page document is already open".into());
    }
    let bytes = read_bounded_bytes(path, Map16PageFile::ENCODED_LEN, "Map16 page document")?;
    *session = Some(Map16PageDocumentController::decode(
        path.to_path_buf(),
        &bytes,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<Map16PageDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        map16_page_edit_script::MAX_SCRIPT_LEN,
        "Map16 page edit",
    )?;
    let edits = map16_page_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no Map16 page document is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&Map16PageDocumentController>) {
    if let Some(controller) = session {
        println!(
            "Map16 page {:04X}: {} tiles — revision {} — {} — undo {} — redo {}",
            controller.value().source_page,
            controller.value().page.tiles.len(),
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
        println!("no Map16 page document open");
    }
}

fn save(
    session: &mut Option<Map16PageDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no Map16 page document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("Map16 page document saved");
    Ok(())
}

fn close(
    session: &mut Option<Map16PageDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no Map16 page document is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "Map16 page document has unsaved changes; use map16-page-save or map16-page-discard"
                .into(),
        );
    }
    *session = None;
    println!("Map16 page document closed");
    Ok(())
}

#[cfg(test)]
#[path = "map16_page_document_shell_tests.rs"]
mod tests;
