use crate::{
    custom_sprite_document_spec, custom_sprite_edit_script, editor_shell::read_bounded_utf8,
    file_persistence, read_bounded_bytes, shell_command::ShellCommand, spec_text,
};
use lm_app::CustomSpriteLibraryController;
use lm_level::{MAX_CUSTOM_SPRITE_SIDECAR_LEN, SpriteLengthTable};
use std::path::Path;

pub(crate) fn execute_custom_sprite_command(
    session: &mut Option<CustomSpriteLibraryController>,
    command: ShellCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ShellCommand::CustomSpriteOpen(path) => open(session, &path),
        ShellCommand::CustomSpriteEdit(path) => edit(session, &path),
        ShellCommand::CustomSpriteUndo => navigate_history(session, true),
        ShellCommand::CustomSpriteRedo => navigate_history(session, false),
        ShellCommand::CustomSpriteStatus => {
            status(session.as_ref());
            Ok(())
        }
        ShellCommand::CustomSpriteSave => save(session),
        ShellCommand::CustomSpriteClose => close(session, false),
        ShellCommand::CustomSpriteDiscard => close(session, true),
        _ => Err("internal non-custom-sprite command dispatch".into()),
    }
}

fn navigate_history(
    session: &mut Option<CustomSpriteLibraryController>,
    undo: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no custom-sprite library is open")?;
    let revision = controller.revision();
    let changed = if undo {
        controller.undo(revision)?
    } else {
        controller.redo(revision)?
    };
    println!(
        "custom-sprite {}: {}",
        if undo { "undo" } else { "redo" },
        if changed { "applied" } else { "unavailable" }
    );
    status(session.as_ref());
    Ok(())
}

pub(crate) fn open(
    session: &mut Option<CustomSpriteLibraryController>,
    spec_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if session.is_some() {
        return Err("a custom-sprite library is already open".into());
    }
    let text = read_bounded_utf8(
        spec_path,
        spec_text::MAX_SPEC_BYTES,
        "custom-sprite document specification",
    )?;
    let spec = custom_sprite_document_spec::parse(&text, spec_path)?;
    let descriptions = spec.data.with_extension("mwt");
    let length_bytes = read_bounded_bytes(
        &spec.sprite_lengths,
        SpriteLengthTable::ENCODED_LEN,
        "sprite-length table",
    )?;
    let lengths = SpriteLengthTable::decode(&length_bytes).map_err(|actual| {
        format!(
            "sprite-length table must contain exactly {} bytes, got {actual}",
            SpriteLengthTable::ENCODED_LEN
        )
    })?;
    let controller = CustomSpriteLibraryController::decode(
        spec.data.clone(),
        descriptions.clone(),
        &read_bounded_bytes(
            &spec.data,
            MAX_CUSTOM_SPRITE_SIDECAR_LEN,
            "custom-sprite data",
        )?,
        &read_bounded_bytes(
            &descriptions,
            MAX_CUSTOM_SPRITE_SIDECAR_LEN,
            "custom-sprite descriptions",
        )?,
        lengths,
    )?;
    println!(
        "custom sprites: {} placements — {}",
        controller.library().entries().len(),
        spec.data.display()
    );
    *session = Some(controller);
    Ok(())
}

pub(crate) fn edit(
    session: &mut Option<CustomSpriteLibraryController>,
    script_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = read_bounded_utf8(
        script_path,
        custom_sprite_edit_script::MAX_SCRIPT_LEN,
        "custom-sprite edit",
    )?;
    let edits = custom_sprite_edit_script::parse(&text)?;
    let controller = session.as_mut().ok_or("no custom-sprite library is open")?;
    controller.apply_edits(controller.revision(), &edits)?;
    status(session.as_ref());
    Ok(())
}

fn status(session: Option<&CustomSpriteLibraryController>) {
    if let Some(controller) = session {
        println!(
            "custom sprites: {} placements — revision {} — {} — undo {} — redo {}",
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
        println!("no custom-sprite library open");
    }
}

pub(crate) fn save(
    session: &mut Option<CustomSpriteLibraryController>,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_mut().ok_or("no custom-sprite library is open")?;
    let snapshot = controller.begin_save()?;
    if let Err(error) = file_persistence::replace_existing_pair(
        (&snapshot.data_path, &snapshot.data),
        (&snapshot.descriptions_path, &snapshot.descriptions),
    ) {
        controller.cancel_save(snapshot.request_id)?;
        return Err(error.into());
    }
    controller.acknowledge_save(snapshot.request_id)?;
    println!("custom-sprite sidecars saved");
    Ok(())
}

pub(crate) fn close(
    session: &mut Option<CustomSpriteLibraryController>,
    discard: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = session.as_ref().ok_or("no custom-sprite library is open")?;
    if controller.is_modified() && !discard {
        return Err(
            "custom-sprite library has unsaved changes; use custom-sprite-save or custom-sprite-discard"
                .into(),
        );
    }
    *session = None;
    println!("custom-sprite library closed");
    Ok(())
}
