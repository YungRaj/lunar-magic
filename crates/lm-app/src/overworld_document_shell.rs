use crate::{
    editor_shell::read_bounded_utf8, file_persistence, overworld_edit_script,
    overworld_render_spec, read_bounded_bytes, shell_command, spec_text,
};
use lm_app::OverworldDocumentController;
use lm_graphics::{GraphicsInterchangeFile, MaterializedAnimationFrame};
use lm_level::Map16SetFile;
use lm_overworld::SpriteAppearanceFile;
use lm_project::CompleteOverworldFile;
use lm_render::{encode_png, render_portable_overworld};
use std::path::Path;

pub(crate) fn execute_overworld_document_command(
    session: &mut Option<OverworldDocumentController>,
    command: shell_command::OverworldDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::OverworldDocumentCommand as DocumentCommand;
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
    session: &mut Option<OverworldDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no complete overworld document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "complete overworld {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn open(
    session: &mut Option<OverworldDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a complete overworld document is already open".into());
    }
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "overworld document open specification",
    )?;
    let spec = overworld_render_spec::parse_overworld_document_open_spec(&text, spec_path)?;
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
        &spec.overworld,
        CompleteOverworldFile::MAX_FILE_LEN,
        "complete overworld",
    )?;
    *session = Some(OverworldDocumentController::decode(
        spec.overworld,
        &bytes,
        spec.maximum_animation_records,
        &modes,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<OverworldDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        path,
        overworld_edit_script::MAX_SCRIPT_LEN,
        "overworld edit script",
    )?;
    let script = overworld_edit_script::parse(&text)?;
    let controller = session
        .as_mut()
        .ok_or("no complete overworld document is open")?;
    controller.apply_edits(
        controller.revision(),
        script.slot,
        &script.palette_ownership,
        &script.edits,
    )?;
    status(session.as_ref());
    Ok(())
}

fn render(
    session: Option<&OverworldDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.ok_or("no complete overworld document is open")?;
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "overworld document render specification",
    )?;
    let spec = overworld_render_spec::parse_overworld_document_render_spec(&text, spec_path)?;
    let map16 = Map16SetFile::decode(&asset(
        &spec.map16,
        Map16SetFile::MAX_FILE_LEN,
        "Map16 set",
    )?)?;
    let graphics = GraphicsInterchangeFile::decode(&asset(
        &spec.graphics,
        GraphicsInterchangeFile::MAX_FILE_LEN,
        "graphics",
    )?)?;
    let appearances = spec
        .appearances
        .as_ref()
        .map(|path| {
            asset(
                path,
                SpriteAppearanceFile::MAX_FILE_LEN,
                "sprite appearances",
            )
            .and_then(|bytes| Ok(SpriteAppearanceFile::decode(&bytes)?))
        })
        .transpose()?;
    let animation_frame = spec
        .animation_frame
        .as_ref()
        .map(|path| {
            asset(
                path,
                MaterializedAnimationFrame::MAX_FILE_LEN,
                "materialized animation frame",
            )
            .and_then(|bytes| Ok(MaterializedAnimationFrame::decode(&bytes)?))
        })
        .transpose()?;
    let world_canvas = render_portable_overworld(
        controller.value(),
        &map16,
        &graphics,
        appearances.as_ref(),
        animation_frame.as_ref(),
        spec.completed_reveals,
    )?;
    let canvas =
        crate::viewport_spec::render(world_canvas, spec.viewport, spec.overlays.as_deref())?;
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "open overworld rendered: {}x{} — revision {} — {}",
        canvas.width(),
        canvas.height(),
        controller.revision(),
        spec.output.display()
    );
    Ok(())
}

fn status(session: Option<&OverworldDocumentController>) {
    if let Some(controller) = session {
        let shape = controller.value().shape;
        println!(
            "complete overworld slot {}: {}x{}, {} sprites, {} reveals — revision {} — {} — undo {} — redo {}",
            controller.value().source_slot,
            shape.width,
            shape.height,
            controller.value().data.sprites.len(),
            controller.value().data.event_reveals.entries.len(),
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
        println!("no complete overworld document open");
    }
}

fn save(
    session: &mut Option<OverworldDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no complete overworld document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("complete overworld document saved");
    Ok(())
}

fn close(
    session: &mut Option<OverworldDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no complete overworld document is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "complete overworld document has unsaved changes; use world-save or world-discard"
                .into(),
        );
    }
    *session = None;
    println!("complete overworld document closed");
    Ok(())
}

fn asset(
    path: &Path,
    maximum: usize,
    kind: &'static str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    read_bounded_bytes(path, maximum, kind)
}
