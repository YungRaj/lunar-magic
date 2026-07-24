use crate::command_types::Command;

pub fn execute(command: &Command) -> Result<bool, Box<dyn std::error::Error>> {
    match command {
        Command::PaletteFile {
            input,
            normalized_output,
            observation,
        } => crate::palette_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::SmwPaletteFile {
            input,
            normalized_output,
            observation,
        } => crate::smw_palette_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::TplPaletteFile {
            input,
            normalized_output,
            observation,
        } => crate::tpl_palette_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::RawPaletteFile {
            input,
            normalized_output,
            observation,
        } => crate::raw_palette_file::execute_palette(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::PaletteMaskFile {
            input,
            normalized_output,
            observation,
        } => crate::raw_palette_file::execute_mask(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        Command::RgbPaletteFile {
            input,
            normalized_output,
            observation,
        } => crate::rgb_palette_file::execute(
            input,
            normalized_output.as_deref(),
            observation.as_deref(),
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}
