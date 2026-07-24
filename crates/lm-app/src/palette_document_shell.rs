use crate::{
    editor_shell::read_bounded_utf8, file_persistence, palette_edit_script, palette_render_spec,
    read_bounded_bytes, shell_command, spec_text,
};
use lm_app::PaletteDocumentController;
use lm_graphics::PaletteInterchangeFile;
use lm_render::{encode_png, render_portable_palette};
use std::path::Path;

pub(crate) fn execute_palette_document_command(
    session: &mut Option<PaletteDocumentController>,
    command: shell_command::PaletteDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::PaletteDocumentCommand as DocumentCommand;
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
    session: &mut Option<PaletteDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no palette document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "palette {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<PaletteDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a palette document is already open".into());
    }
    *session = Some(PaletteDocumentController::decode(
        path.to_path_buf(),
        &read_bounded_bytes(path, PaletteInterchangeFile::MAX_FILE_LEN, "palette")?,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<PaletteDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        palette_edit_script::MAX_SCRIPT_LEN,
        "palette edit script",
    )?;
    let script = palette_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no palette document is open")?;
    controller.apply_edits(controller.revision(), &script.ownership, &script.edits)?;
    status(session.as_ref());
    Ok(())
}

fn render(
    session: Option<&PaletteDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.ok_or("no palette document is open")?;
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "palette document render specification",
    )?;
    let spec = palette_render_spec::parse_palette_document_render_spec(&text, spec_path)?;
    let canvas = crate::viewport_spec::render(
        render_portable_palette(controller.value(), spec.columns, spec.cell_size)?,
        spec.viewport,
        spec.overlays.as_deref(),
    )?;
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "open palette rendered: {}x{} — revision {} — {}",
        canvas.width(),
        canvas.height(),
        controller.revision(),
        spec.output.display()
    );
    Ok(())
}

fn status(session: Option<&PaletteDocumentController>) {
    if let Some(controller) = session {
        println!(
            "palette {}: {} colors — revision {} — {} — undo {} — redo {}",
            controller.value().source_palette,
            controller.value().palette.colors.len(),
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
        println!("no palette document open");
    }
}

fn save(session: &mut Option<PaletteDocumentController>) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no palette document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("palette document saved");
    Ok(())
}

fn close(
    session: &mut Option<PaletteDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no palette document is open")?;
    if controller.is_modified() && !discard {
        return Err("palette document has unsaved changes; use pal-save or pal-discard".into());
    }
    *session = None;
    println!("palette document closed");
    Ok(())
}
