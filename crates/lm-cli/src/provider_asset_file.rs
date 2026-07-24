use crate::command_types::Command;

pub fn execute(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::Layer3PlaneFile {
            input,
            normalized_output,
            observation,
        } => crate::layer3_plane_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::AnimationFrameFile {
            input,
            normalized_output,
            observation,
        } => crate::animation_frame_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::AppearanceFile {
            input,
            normalized_output,
            observation,
        } => crate::appearance_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::OverworldAppearanceFile {
            input,
            normalized_output,
            observation,
        } => crate::overworld_appearance_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::NativeLevelFile {
            input,
            sprite_lengths,
            normalized_output,
            observation,
        } => crate::native_level_file::execute(
            input,
            sprite_lengths.as_deref(),
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}
