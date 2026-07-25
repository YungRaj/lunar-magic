use super::{
    LevelHeaderField, ScriptEditor, ShellCommand, ShellCommandError, hex_usize, hex_value,
};
use std::path::PathBuf;

pub(super) fn parse_owned_editor_command(
    command: &str,
    argument: &str,
) -> Option<Result<ShellCommand, ShellCommandError>> {
    match command {
        "exanimation-edit-owned" => Some(parse_owned_exanimation_edit_script(argument)),
        "graphics-edit-owned" => Some(parse_owned_graphics_edit_script(argument)),
        "level-edit-owned" => Some(parse_owned_level_edit_script(argument)),
        "map16-edit-owned" => Some(parse_owned_map16_edit_script(argument)),
        "native-assets-edit-owned" => Some(parse_owned_native_assets_edit_script(argument)),
        "overworld-edit-owned" => Some(parse_owned_overworld_edit_script(argument)),
        "palette-edit-owned" => Some(parse_owned_palette_edit_script(argument)),
        _ => None,
    }
}

fn parse_owned_native_assets_edit_script(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let (prefix, manifest) = argument.trim().rsplit_once(char::is_whitespace).ok_or(
        ShellCommandError::MissingArgument("native-assets-edit-owned"),
    )?;
    if manifest.is_empty() {
        return Err(ShellCommandError::MissingArgument(
            "native-assets-edit-owned",
        ));
    }
    let (script, search_start, search_end) =
        parse_script_path_and_range(prefix, "native-assets-edit-owned")?;
    Ok(ShellCommand::ApplyOwnedEditorScript {
        editor: ScriptEditor::NativeAssets,
        script,
        ownership_manifest: PathBuf::from(manifest),
        search_start,
        search_end,
    })
}

fn parse_owned_overworld_edit_script(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let (prefix, manifest) = argument
        .trim()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument("overworld-edit-owned"))?;
    if manifest.is_empty() {
        return Err(ShellCommandError::MissingArgument("overworld-edit-owned"));
    }
    let (script, search_start, search_end) =
        parse_script_path_and_range(prefix, "overworld-edit-owned")?;
    Ok(ShellCommand::ApplyOwnedEditorScript {
        editor: ScriptEditor::Overworld,
        script,
        ownership_manifest: PathBuf::from(manifest),
        search_start,
        search_end,
    })
}

fn parse_owned_map16_edit_script(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let (prefix, manifest) = argument
        .trim()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument("map16-edit-owned"))?;
    if manifest.is_empty() {
        return Err(ShellCommandError::MissingArgument("map16-edit-owned"));
    }
    let (script, search_start, search_end) =
        parse_script_path_and_range(prefix, "map16-edit-owned")?;
    Ok(ShellCommand::ApplyOwnedEditorScript {
        editor: ScriptEditor::Map16,
        script,
        ownership_manifest: PathBuf::from(manifest),
        search_start,
        search_end,
    })
}

fn parse_owned_level_edit_script(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let (prefix, manifest) = argument
        .trim()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument("level-edit-owned"))?;
    if manifest.is_empty() {
        return Err(ShellCommandError::MissingArgument("level-edit-owned"));
    }
    let (script, search_start, search_end) =
        parse_script_path_and_range(prefix, "level-edit-owned")?;
    Ok(ShellCommand::ApplyOwnedEditorScript {
        editor: ScriptEditor::Level,
        script,
        ownership_manifest: PathBuf::from(manifest),
        search_start,
        search_end,
    })
}

fn parse_owned_exanimation_edit_script(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let (prefix, manifest) = argument
        .trim()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument("exanimation-edit-owned"))?;
    if manifest.is_empty() {
        return Err(ShellCommandError::MissingArgument("exanimation-edit-owned"));
    }
    let (script, search_start, search_end) =
        parse_script_path_and_range(prefix, "exanimation-edit-owned")?;
    Ok(ShellCommand::ApplyOwnedEditorScript {
        editor: ScriptEditor::ExAnimation,
        script,
        ownership_manifest: PathBuf::from(manifest),
        search_start,
        search_end,
    })
}

pub(super) fn parse_rom_expansion(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let values = argument.split_whitespace().collect::<Vec<_>>();
    let [target, fill] = values.as_slice() else {
        return Err(if values.len() < 2 {
            ShellCommandError::MissingArgument("rom-expand")
        } else {
            ShellCommandError::UnexpectedArgument("rom-expand")
        });
    };
    let fill = u8::try_from(hex_value(fill, "rom-expand")?).map_err(|_| {
        ShellCommandError::InvalidRange {
            command: "rom-expand",
            value: (*fill).into(),
        }
    })?;
    Ok(ShellCommand::ExpandRom {
        target_logical_len: hex_usize(target, "rom-expand")?,
        fill,
    })
}

pub(super) fn parse_graphics_recompression(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let values: Vec<_> = argument.split_whitespace().collect();
    let [target, search_start, search_end] = values.as_slice() else {
        return Err(if values.len() < 3 {
            ShellCommandError::MissingArgument("graphics-recompress")
        } else {
            ShellCommandError::UnexpectedArgument("graphics-recompress")
        });
    };
    let target = match *target {
        "lz2" => lm_project::GraphicsCompression::Lz2,
        "lz3" => lm_project::GraphicsCompression::Lz3,
        unknown => {
            return Err(ShellCommandError::InvalidGraphicsCompression(
                unknown.into(),
            ));
        }
    };
    Ok(ShellCommand::MigrateGraphicsCompression {
        target,
        search_start: hex_usize(search_start, "graphics-recompress")?,
        search_end: hex_usize(search_end, "graphics-recompress")?,
    })
}

pub(super) fn parse_exanimation_edit_script(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let (script, search_start, search_end) =
        parse_script_path_and_range(argument, "exanimation-edit")?;
    Ok(ShellCommand::ApplyEditorScript {
        editor: ScriptEditor::ExAnimation,
        script,
        search_start,
        search_end,
    })
}

pub(super) fn parse_overworld_edit_script(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let (script, search_start, search_end) =
        parse_script_path_and_range(argument, "overworld-edit")?;
    Ok(ShellCommand::ApplyEditorScript {
        editor: ScriptEditor::Overworld,
        script,
        search_start,
        search_end,
    })
}

pub(super) fn parse_level_edit_script(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let (script, search_start, search_end) = parse_script_path_and_range(argument, "level-edit")?;
    Ok(ShellCommand::ApplyEditorScript {
        editor: ScriptEditor::Level,
        script,
        search_start,
        search_end,
    })
}

pub(super) fn parse_map16_edit_script(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let (script, search_start, search_end) = parse_script_path_and_range(argument, "map16-edit")?;
    Ok(ShellCommand::ApplyEditorScript {
        editor: ScriptEditor::Map16,
        script,
        search_start,
        search_end,
    })
}

pub(super) fn parse_palette_edit_script(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let (script, search_start, search_end) = parse_script_path_and_range(argument, "palette-edit")?;
    Ok(ShellCommand::ApplyEditorScript {
        editor: ScriptEditor::Palette,
        script,
        search_start,
        search_end,
    })
}

pub(super) fn parse_graphics_edit_script(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let (script, search_start, search_end) =
        parse_script_path_and_range(argument, "graphics-edit")?;
    Ok(ShellCommand::ApplyEditorScript {
        editor: ScriptEditor::Graphics,
        script,
        search_start,
        search_end,
    })
}

pub(super) fn parse_owned_graphics_edit_script(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let (prefix, manifest) = argument
        .trim()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument("graphics-edit-owned"))?;
    if manifest.is_empty() {
        return Err(ShellCommandError::MissingArgument("graphics-edit-owned"));
    }
    let (script, search_start, search_end) =
        parse_script_path_and_range(prefix, "graphics-edit-owned")?;
    Ok(ShellCommand::ApplyOwnedEditorScript {
        editor: ScriptEditor::Graphics,
        script,
        ownership_manifest: PathBuf::from(manifest),
        search_start,
        search_end,
    })
}

pub(super) fn parse_owned_palette_edit_script(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let (prefix, manifest) = argument
        .trim()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument("palette-edit-owned"))?;
    if manifest.is_empty() {
        return Err(ShellCommandError::MissingArgument("palette-edit-owned"));
    }
    let (script, search_start, search_end) =
        parse_script_path_and_range(prefix, "palette-edit-owned")?;
    Ok(ShellCommand::ApplyOwnedEditorScript {
        editor: ScriptEditor::Palette,
        script,
        ownership_manifest: PathBuf::from(manifest),
        search_start,
        search_end,
    })
}

fn parse_script_path_and_range(
    argument: &str,
    command: &'static str,
) -> Result<(PathBuf, usize, usize), ShellCommandError> {
    let (prefix, search_end) = argument
        .trim()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument(command))?;
    let (script, search_start) = prefix
        .trim_end()
        .rsplit_once(char::is_whitespace)
        .ok_or(ShellCommandError::MissingArgument(command))?;
    let script = script.trim();
    if script.is_empty() || search_start.is_empty() || search_end.is_empty() {
        return Err(ShellCommandError::MissingArgument(command));
    }
    Ok((
        PathBuf::from(script),
        hex_usize(search_start, command)?,
        hex_usize(search_end, command)?,
    ))
}

pub(super) fn parse_native_assets_edit_script(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let (script, search_start, search_end) =
        parse_script_path_and_range(argument, "native-assets-edit")?;
    Ok(ShellCommand::ApplyEditorScript {
        editor: ScriptEditor::NativeAssets,
        script,
        search_start,
        search_end,
    })
}

pub(super) fn parse_level_header_edit(argument: &str) -> Result<ShellCommand, ShellCommandError> {
    let values: Vec<_> = argument.split_whitespace().collect();
    let [field, value, search_start, search_end] = values.as_slice() else {
        return Err(if values.len() < 4 {
            ShellCommandError::MissingArgument("level-header")
        } else {
            ShellCommandError::UnexpectedArgument("level-header")
        });
    };
    let field = match *field {
        "background-palette" => LevelHeaderField::BackgroundPalette,
        "mode" => LevelHeaderField::LevelMode,
        "background-color" => LevelHeaderField::BackgroundColor,
        "sprite-tileset" => LevelHeaderField::SpriteTileset,
        "sprite-palette" => LevelHeaderField::SpritePalette,
        "foreground-palette" => LevelHeaderField::ForegroundPalette,
        "object-tileset" => LevelHeaderField::ObjectTileset,
        unknown => return Err(ShellCommandError::InvalidLevelHeaderField(unknown.into())),
    };
    let value = u8::try_from(hex_value(value, "level-header")?).map_err(|_| {
        ShellCommandError::InvalidRange {
            command: "level-header",
            value: (*value).into(),
        }
    })?;
    Ok(ShellCommand::EditLevelHeader {
        field,
        value,
        search_start: usize::try_from(hex_value(search_start, "level-header")?).map_err(|_| {
            ShellCommandError::InvalidRange {
                command: "level-header",
                value: (*search_start).into(),
            }
        })?,
        search_end: usize::try_from(hex_value(search_end, "level-header")?).map_err(|_| {
            ShellCommandError::InvalidRange {
                command: "level-header",
                value: (*search_end).into(),
            }
        })?,
    })
}

pub(super) fn parse_expanded_settings_word(
    argument: &str,
) -> Result<ShellCommand, ShellCommandError> {
    let values: Vec<_> = argument.split_whitespace().collect();
    let [index, value] = values.as_slice() else {
        return Err(if values.len() < 2 {
            ShellCommandError::MissingArgument("expanded-settings-word")
        } else {
            ShellCommandError::UnexpectedArgument("expanded-settings-word")
        });
    };
    let index = hex_usize(index, "expanded-settings-word")?;
    let value = u16::try_from(hex_value(value, "expanded-settings-word")?).map_err(|_| {
        ShellCommandError::InvalidRange {
            command: "expanded-settings-word",
            value: (*value).into(),
        }
    })?;
    Ok(ShellCommand::EditExpandedSettingsWord { index, value })
}
