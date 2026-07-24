use crate::{
    editor_shell::read_bounded_utf8, file_persistence, graphics_edit_script, graphics_render_spec,
    read_bounded_bytes, shell_command, spec_text,
};
use lm_app::GraphicsDocumentController;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_render::{encode_png, render_portable_graphics};
use std::path::Path;

pub(crate) fn execute_graphics_document_command(
    session: &mut Option<GraphicsDocumentController>,
    command: shell_command::GraphicsDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::GraphicsDocumentCommand as DocumentCommand;
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
    session: &mut Option<GraphicsDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no graphics document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "graphics {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<GraphicsDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a graphics document is already open".into());
    }
    *session = Some(GraphicsDocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(path, GraphicsInterchangeFile::MAX_FILE_LEN, "graphics")?,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<GraphicsDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        graphics_edit_script::MAX_SCRIPT_LEN,
        "graphics edit script",
    )?;
    let script = graphics_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no graphics document is open")?;
    controller.apply_edits(controller.revision(), &script.ownership, &script.edits)?;
    status(session.as_ref());
    Ok(())
}

fn render(
    session: Option<&GraphicsDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.ok_or("no graphics document is open")?;
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "graphics document render specification",
    )?;
    let spec = graphics_render_spec::parse_graphics_document_render_spec(&text, spec_path)?;
    let palette = PaletteInterchangeFile::decode(&read_bounded_bytes(
        &spec.palette,
        PaletteInterchangeFile::MAX_FILE_LEN,
        "palette",
    )?)?;
    let canvas = crate::viewport_spec::render(
        render_portable_graphics(controller.value(), &palette, spec.palette_row, spec.columns)?,
        spec.viewport,
        spec.overlays.as_deref(),
    )?;
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "open graphics rendered: {}x{} — revision {} — {}",
        canvas.width(),
        canvas.height(),
        controller.revision(),
        spec.output.display()
    );
    Ok(())
}

fn status(session: Option<&GraphicsDocumentController>) {
    if let Some(controller) = session {
        println!(
            "graphics slot {}: {} tiles — revision {} — {} — undo {} — redo {}",
            controller.value().source_slot,
            controller.value().graphics.tiles.len(),
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
        println!("no graphics document open");
    }
}

fn save(
    session: &mut Option<GraphicsDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no graphics document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("graphics document saved");
    Ok(())
}

fn close(
    session: &mut Option<GraphicsDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no graphics document is open")?;
    if controller.is_modified() && !discard {
        return Err("graphics document has unsaved changes; use gfx-save or gfx-discard".into());
    }
    *session = None;
    println!("graphics document closed");
    Ok(())
}
