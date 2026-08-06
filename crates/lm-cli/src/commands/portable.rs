use crate::args::Command;

pub(super) fn execute(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    if execute_focused(command)? {
        return Ok(true);
    }
    match command {
        Command::EditorOverlayFile {
            input,
            normalized_output,
            observation,
        } => crate::editor_overlay_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::CompleteLevel {
            input,
            normalized_output,
            observation,
        } => crate::complete_level::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::Layer3File {
            input,
            normalized_output,
            observation,
        } => crate::layer3_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::CustomObjectLibrary {
            data,
            descriptions,
            normalized_outputs,
            observation,
        } => crate::custom_object_library::execute(
            data,
            descriptions,
            normalized_outputs
                .as_ref()
                .map(|(data, descriptions)| (data.as_path(), descriptions.as_path())),
            observation.as_deref(),
        )?,
        Command::CustomSpriteLibrary {
            data,
            descriptions,
            sprite_lengths,
            normalized_outputs,
            observation,
        } => crate::custom_sprite_library::execute(
            data,
            descriptions,
            sprite_lengths,
            normalized_outputs
                .as_ref()
                .map(|(data, descriptions)| (data.as_path(), descriptions.as_path())),
            observation.as_deref(),
        )?,
        Command::CompleteMap16 {
            input,
            normalized_output,
            observation,
        } => crate::map16_set_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::OverworldPath {
            input,
            normalized_output,
            observation,
        } => crate::overworld_path::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::OverworldMetadata {
            input,
            normalized_output,
            observation,
        } => crate::overworld_metadata::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn execute_focused(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(execute_specialized(command)?
        || execute_revision_patch(command)?
        || crate::layer3_workspace::execute_command(command)?
        || crate::graphics_remap::execute_command(command)?
        || execute_scene_tilemap(command)?)
}

fn execute_scene_tilemap(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::LayerTilemapFile {
            input,
            normalized_output,
            observation,
        } => crate::layer_tilemap_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::CreditsTilemapFile {
            input,
            normalized_output,
            observation,
        } => crate::credits_tilemap_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::OverworldEventFile {
            input,
            normalized_output,
            observation,
        } => crate::overworld_event_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn execute_specialized(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(crate::dsc_sidecar::execute_command(command)?
        || crate::native_overworld_appearance_file::execute_command(command)?
        || crate::native_map16_sidecar::execute_command(command)?
        || crate::lm16_map16_file::execute_command(command)?
        || crate::ownership_file::execute_command(command)?
        || crate::portable_render::execute(command)?
        || crate::portable_asset_file::execute(command)?)
}

fn execute_revision_patch(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    let Command::RevisionPatchFile {
        input,
        normalized_output,
        observation,
    } = command
    else {
        return Ok(false);
    };
    crate::revision_patch_file::execute(
        input,
        normalized_output.as_deref(),
        observation.as_deref(),
    )?;
    Ok(true)
}
