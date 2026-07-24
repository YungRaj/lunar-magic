use crate::{arg_values::ArgsError, command_types::Command};
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Result<Option<Command>, ArgsError> {
    if let Some(command) = parse_level(args, text)? {
        return Ok(Some(command));
    }
    let [command, _, _, _, _, page, first, suppressed, second, _] = text else {
        return Ok(None);
    };
    if command != "render-map16-dsc" {
        return Ok(None);
    }
    Ok(Some(Command::RenderMap16Dsc {
        graphics: PathBuf::from(&args[1]),
        palette: PathBuf::from(&args[2]),
        map16: PathBuf::from(&args[3]),
        dsc: PathBuf::from(&args[4]),
        page: parse_usize(page, "page")?,
        first_feature: parse_switch(first, "first feature")?,
        first_suppressed: parse_switch(suppressed, "first-feature suppression")?,
        second_feature: parse_switch(second, "second feature")?,
        output: PathBuf::from(&args[9]),
    }))
}

fn parse_level(args: &[OsString], text: &[Cow<'_, str>]) -> Result<Option<Command>, ArgsError> {
    let [
        command,
        _,
        _,
        _,
        _,
        appearances,
        layer3,
        _,
        custom,
        markers,
        first,
        suppressed,
        second,
        mode,
        width1,
        height1,
        width2,
        height2,
        _,
    ] = text
    else {
        return Ok(None);
    };
    if command != "render-level-dsc" {
        return Ok(None);
    }
    Ok(Some(Command::RenderLevelDsc {
        level: PathBuf::from(&args[1]),
        map16: PathBuf::from(&args[2]),
        graphics: PathBuf::from(&args[3]),
        palette: PathBuf::from(&args[4]),
        appearances: (appearances.as_ref() != "none").then(|| PathBuf::from(&args[5])),
        layer3_plane: (layer3.as_ref() != "none").then(|| PathBuf::from(&args[6])),
        dsc: PathBuf::from(&args[7]),
        custom_display: parse_switch(custom, "custom-display")?,
        special_markers: parse_switch(markers, "special-marker")?,
        first_feature: parse_switch(first, "first feature")?,
        first_suppressed: parse_switch(suppressed, "first-feature suppression")?,
        second_feature: parse_switch(second, "second feature")?,
        level_mode: u8::try_from(parse_usize(mode, "level mode")?)
            .map_err(|_| ArgsError("level mode does not fit u8".into()))?,
        layer1_width: parse_usize(width1, "layer 1 width")?,
        layer1_height: parse_usize(height1, "layer 1 height")?,
        layer2_width: parse_usize(width2, "layer 2 width")?,
        layer2_height: parse_usize(height2, "layer 2 height")?,
        output: PathBuf::from(&args[18]),
    }))
}

fn parse_usize(value: &str, name: &str) -> Result<usize, ArgsError> {
    value
        .parse()
        .map_err(|_| ArgsError(format!("invalid {name} {value}")))
}

fn parse_switch(value: &str, name: &str) -> Result<bool, ArgsError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(ArgsError(format!("invalid {name} switch {value}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_page_switches_and_paths() {
        let args: Vec<OsString> = [
            "render-map16-dsc",
            "gfx",
            "palette",
            "map16",
            "display.dsc",
            "3",
            "1",
            "0",
            "1",
            "out.png",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args, &text).unwrap(),
            Some(Command::RenderMap16Dsc {
                page: 3,
                first_feature: true,
                first_suppressed: false,
                second_feature: true,
                ..
            })
        ));
        let mut invalid = text.clone();
        invalid[6] = Cow::Borrowed("yes");
        assert!(parse(&args, &invalid).is_err());
    }

    #[test]
    fn parses_dsc_level_context_and_optional_assets() {
        let args: Vec<OsString> = [
            "render-level-dsc",
            "level",
            "map16",
            "gfx",
            "palette",
            "none",
            "layer3",
            "display.dsc",
            "1",
            "0",
            "1",
            "0",
            "1",
            "13",
            "16",
            "27",
            "0",
            "0",
            "out.png",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args, &text).unwrap(),
            Some(Command::RenderLevelDsc {
                appearances: None,
                layer3_plane: Some(_),
                custom_display: true,
                level_mode: 13,
                layer1_width: 16,
                ..
            })
        ));
    }
}
