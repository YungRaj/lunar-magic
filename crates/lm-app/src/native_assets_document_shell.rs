use crate::{
    editor_shell::read_bounded_utf8, file_persistence, native_assets_document_spec,
    native_assets_edit_loader, palette_render_spec, read_bounded_bytes, shell_command, spec_text,
};
use lm_app::{NativeLevelAssetsDocumentController, RevisionProfile};
use lm_graphics::{PaletteInterchangeFile, PaletteOwnership};
use lm_project::NativeLevelAssetsFile;
use lm_render::{encode_png, render_portable_palette};
use std::{fs, path::Path};

pub(crate) fn execute(
    session: &mut Option<NativeLevelAssetsDocumentController>,
    command: shell_command::NativeAssetsDocumentCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    use shell_command::NativeAssetsDocumentCommand as DocumentCommand;
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
    session: &mut Option<NativeLevelAssetsDocumentController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no native-assets document is open")?;
    let changed = if undo {
        controller.undo(controller.revision())?
    } else {
        controller.redo(controller.revision())?
    };
    println!(
        "native-assets {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

fn render(
    session: Option<&NativeLevelAssetsDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.ok_or("no native-assets document is open")?;
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "native-assets palette render specification",
    )?;
    let spec = palette_render_spec::parse_palette_document_render_spec(&text, spec_path)?;
    let palette = PaletteInterchangeFile {
        source_palette: controller.value().source_slot,
        palette: controller.value().assets.palette.clone(),
    };
    let canvas = crate::viewport_spec::render(
        render_portable_palette(&palette, spec.columns, spec.cell_size)?,
        spec.viewport,
        spec.overlays.as_deref(),
    )?;
    file_persistence::write_new(&spec.output, &encode_png(&canvas)?)?;
    println!(
        "native-assets palette rendered: {}x{} — revision {} — {}",
        canvas.width(),
        canvas.height(),
        controller.revision(),
        spec.output.display()
    );
    Ok(())
}

fn open(
    session: &mut Option<NativeLevelAssetsDocumentController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a native-assets document is already open".into());
    }
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "native-assets document open specification",
    )?;
    let spec = native_assets_document_spec::parse(&text, spec_path)?;
    let profile = RevisionProfile::read_from(fs::File::open(&spec.profile)?)?;
    let bytes = read_bounded_bytes(
        &spec.document,
        NativeLevelAssetsFile::MAX_FILE_LEN,
        "native-assets document",
    )?;
    *session = Some(NativeLevelAssetsDocumentController::decode(
        spec.document,
        &bytes,
        profile.sprite_lengths,
        profile.exanimation.maximum_records,
        &profile.exanimation_double_size_modes,
    )?);
    status(session.as_ref());
    Ok(())
}

fn edit(
    session: &mut Option<NativeLevelAssetsDocumentController>,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let loaded = native_assets_edit_loader::load(path)?;
    if !loaded.map16_edits.is_empty() || !loaded.entrance_edits.is_empty() {
        return Err("Map16 and entrance edits require a ROM-backed native-assets session".into());
    }
    let controller = session
        .as_mut()
        .ok_or("no native-assets document is open")?;
    let ownership = loaded.palette_ownership.unwrap_or_else(|| {
        PaletteOwnership::editable(controller.value().assets.palette.colors.len())
    });
    controller.apply_edits(controller.revision(), &loaded.edits, &ownership)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&NativeLevelAssetsDocumentController>) {
    if let Some(controller) = session {
        let assets = &controller.value().assets;
        println!(
            "native assets {:03X}: {} objects, {} sprite tokens, {} colors, {} animations, settings {} — revision {} — {} — undo {} — redo {}",
            controller.value().source_slot,
            assets.level.layer1.objects.records.len(),
            assets.level.sprites.tokens.len(),
            assets.palette.colors.len(),
            assets.exanimation.records.len(),
            if assets.expanded_settings.is_some() {
                "present"
            } else {
                "absent"
            },
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
        println!("no native-assets document open");
    }
}

fn save(
    session: &mut Option<NativeLevelAssetsDocumentController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_mut()
        .ok_or("no native-assets document is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing(&snapshot.path, &snapshot.bytes) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("native-assets document saved");
    Ok(())
}

fn close(
    session: &mut Option<NativeLevelAssetsDocumentController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session
        .as_ref()
        .ok_or("no native-assets document is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "native-assets document has unsaved changes; use native-assets-save or native-assets-discard"
                .into(),
        );
    }
    *session = None;
    println!("native-assets document closed");
    Ok(())
}

#[cfg(test)]
#[path = "native_assets_document_shell_tests.rs"]
mod tests;
