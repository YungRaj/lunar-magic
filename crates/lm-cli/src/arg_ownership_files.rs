use crate::{arg_values::ArgsError, command_types::Command};
use std::{ffi::OsString, path::PathBuf};

pub fn parse(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    let Some(command) = text.first() else {
        return Ok(None);
    };
    if !matches!(
        command.as_ref(),
        "graphics-ownership-file" | "palette-ownership-file"
    ) {
        return Ok(None);
    }
    let paths = match text {
        [command, _]
            if matches!(
                command.as_ref(),
                "graphics-ownership-file" | "palette-ownership-file"
            ) =>
        {
            Some((PathBuf::from(&args[1]), None, None))
        }
        [command, _, _]
            if matches!(
                command.as_ref(),
                "graphics-ownership-file" | "palette-ownership-file"
            ) =>
        {
            Some((PathBuf::from(&args[1]), Some(PathBuf::from(&args[2])), None))
        }
        [command, _, _, _]
            if matches!(
                command.as_ref(),
                "graphics-ownership-file" | "palette-ownership-file"
            ) =>
        {
            Some((
                PathBuf::from(&args[1]),
                Some(PathBuf::from(&args[2])),
                Some(PathBuf::from(&args[3])),
            ))
        }
        _ => None,
    };
    let (input, normalized_output, observation) = paths.ok_or_else(|| {
        ArgsError(format!(
            "usage: {command} INPUT [NORMALIZED_OUTPUT [OBSERVATION]]"
        ))
    })?;
    Ok(Some(match text[0].as_ref() {
        "graphics-ownership-file" => Command::GraphicsOwnershipFile {
            input,
            normalized_output,
            observation,
        },
        "palette-ownership-file" => Command::PaletteOwnershipFile {
            input,
            normalized_output,
            observation,
        },
        _ => unreachable!("guarded ownership command"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_kinds_and_optional_outputs() {
        for name in ["graphics-ownership-file", "palette-ownership-file"] {
            for values in [
                vec![name, "input"],
                vec![name, "input", "normalized"],
                vec![name, "input", "normalized", "observation"],
            ] {
                let args = values.iter().map(OsString::from).collect::<Vec<_>>();
                let text = values
                    .iter()
                    .map(|value| (*value).into())
                    .collect::<Vec<_>>();
                assert!(parse(&args, &text).unwrap().is_some());
            }
        }
        let bad = [OsString::from("graphics-ownership-file")];
        let text = ["graphics-ownership-file".into()];
        assert!(parse(&bad, &text).is_err());
    }
}
