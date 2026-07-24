use crate::{
    arg_values::{ArgsError, parse_number},
    command_types::Command,
};
use std::{ffi::OsString, path::PathBuf};

pub fn parse(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    if let Some(command) = parse_focused_portable_command(args, text)? {
        return Ok(Some(command));
    }
    Ok(match text {
        [command, _] if command == "level-bundle" => Some(Command::CompleteLevel {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "level-bundle" => Some(Command::CompleteLevel {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "level-bundle" => Some(Command::CompleteLevel {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        }),
        [command, _, _, _] if command == "level-bundle-edit" => Some(Command::EditCompleteLevel {
            input: PathBuf::from(&args[1]),
            script: PathBuf::from(&args[2]),
            output: PathBuf::from(&args[3]),
        }),
        [command, _] if command == "layer3-file" => Some(Command::Layer3File {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "layer3-file" => Some(Command::Layer3File {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "layer3-file" => Some(Command::Layer3File {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        }),
        [command, _] if command == "map16-set-file" => Some(Command::CompleteMap16 {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "map16-set-file" => Some(Command::CompleteMap16 {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "map16-set-file" => Some(Command::CompleteMap16 {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        }),
        [command, _, _, _, _] if command == "render-map16-page" => Some(Command::RenderMap16Page {
            graphics: PathBuf::from(&args[1]),
            palette: PathBuf::from(&args[2]),
            page: PathBuf::from(&args[3]),
            output: PathBuf::from(&args[4]),
        }),
        _ => None,
    })
}

fn parse_focused_portable_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    if let Some(command) = crate::arg_asset_renders::parse(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_exanimation_file::parse(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_overworld_file::parse(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_editor_overlay_file::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_expanded_settings_file::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_revision_patch_file::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_layer3_workspace::parse(args, text)? {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_layer_tilemap_file::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_credits_tilemap_file::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_overworld_event_file::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_graphics_remap::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_native_assets_file::parse(args, text) {
        return Ok(Some(command));
    }
    if let Some(command) = crate::arg_ownership_files::parse(args, text)? {
        return Ok(Some(command));
    }
    Ok(crate::arg_native_level_file::parse(args, text)
        .or_else(|| crate::arg_appearance_files::parse(args, text))
        .or_else(|| crate::arg_overworld_files::parse(args, text))
        .or_else(|| crate::arg_palette_files::parse(args, text))
        .or_else(|| crate::arg_portable_assets::parse(args, text))
        .or(parse_overworld_render(args, text)?)
        .or(parse_level_render(args, text)?))
}

fn parse_level_render(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    let (appearances, layer3_plane, dimensions, output_index) = match text {
        [command, _, _, _, _, width1, height1, width2, height2, _] if command == "render-level" => {
            (None, None, [width1, height1, width2, height2], 9)
        }
        [
            command,
            _,
            _,
            _,
            _,
            appearance,
            width1,
            height1,
            width2,
            height2,
            _,
        ] if command == "render-level" => (
            (appearance.as_ref() != "none").then(|| PathBuf::from(&args[5])),
            None,
            [width1, height1, width2, height2],
            10,
        ),
        [
            command,
            _,
            _,
            _,
            _,
            appearance,
            layer3,
            width1,
            height1,
            width2,
            height2,
            _,
        ] if command == "render-level" => (
            (appearance.as_ref() != "none").then(|| PathBuf::from(&args[5])),
            (layer3.as_ref() != "none").then(|| PathBuf::from(&args[6])),
            [width1, height1, width2, height2],
            11,
        ),
        _ => return Ok(None),
    };
    let parse_dimension = |value: &str, name: &str| {
        usize::try_from(parse_number(value)?)
            .map_err(|_| ArgsError(format!("{name} does not fit usize")))
    };
    Ok(Some(Command::RenderLevel {
        level: PathBuf::from(&args[1]),
        map16: PathBuf::from(&args[2]),
        graphics: PathBuf::from(&args[3]),
        palette: PathBuf::from(&args[4]),
        appearances,
        layer3_plane,
        layer1_width: parse_dimension(dimensions[0], "layer 1 width")?,
        layer1_height: parse_dimension(dimensions[1], "layer 1 height")?,
        layer2_width: parse_dimension(dimensions[2], "layer 2 width")?,
        layer2_height: parse_dimension(dimensions[3], "layer 2 height")?,
        output: PathBuf::from(&args[output_index]),
    }))
}

fn parse_overworld_render(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    let (records, appearances, animation_frame, reveals, output_index) = match text {
        [command, _, _, records, _, _, reveals, _] if command == "render-overworld" => {
            (records, None, None, reveals, 7)
        }
        [command, _, _, records, _, _, appearance, reveals, _] if command == "render-overworld" => {
            (
                records,
                (appearance.as_ref() != "none").then(|| PathBuf::from(&args[6])),
                None,
                reveals,
                8,
            )
        }
        [command, _, _, records, _, _, appearance, frame, reveals, _]
            if command == "render-overworld" =>
        {
            (
                records,
                (appearance.as_ref() != "none").then(|| PathBuf::from(&args[6])),
                (frame.as_ref() != "none").then(|| PathBuf::from(&args[7])),
                reveals,
                9,
            )
        }
        _ => return Ok(None),
    };
    Ok(Some(Command::RenderOverworld {
        overworld: PathBuf::from(&args[1]),
        size_modes: PathBuf::from(&args[2]),
        maximum_animation_records: usize::try_from(parse_number(records)?)
            .map_err(|_| ArgsError("animation record limit does not fit usize".into()))?,
        map16: PathBuf::from(&args[4]),
        graphics: PathBuf::from(&args[5]),
        appearances,
        animation_frame,
        completed_reveals: usize::try_from(parse_number(reveals)?)
            .map_err(|_| ArgsError("completed reveal count does not fit usize".into()))?,
        output: PathBuf::from(&args[output_index]),
    }))
}
