use crate::command_types::Command;

pub fn execute(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    if crate::provider_asset_file::execute(command)? {
        return Ok(true);
    }
    if crate::palette_asset_file::execute(command)? {
        return Ok(true);
    }
    match command {
        Command::GraphicsFile {
            input,
            normalized_output,
            observation,
        } => crate::graphics_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::ExAnimationFile {
            input,
            size_modes,
            maximum_records,
            normalized_output,
            observation,
        } => crate::exanimation_file::execute(
            input,
            size_modes,
            *maximum_records,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::OverworldFile {
            input,
            size_modes,
            maximum_animation_records,
            normalized_output,
            observation,
        } => crate::overworld_file::execute(
            input,
            size_modes,
            *maximum_animation_records,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::Map16PageFile {
            input,
            normalized_output,
            observation,
        } => crate::map16_page_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}
