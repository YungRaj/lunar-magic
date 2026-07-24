use crate::command_types::Command;
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Option<Command> {
    let command = match text {
        [command, _] if command == "dsc-sidecar" => Command::DscSidecar {
            input: PathBuf::from(&args[1]),
            lossless_output: None,
            observation: None,
        },
        [command, _, _] if command == "dsc-sidecar" => Command::DscSidecar {
            input: PathBuf::from(&args[1]),
            lossless_output: Some(PathBuf::from(&args[2])),
            observation: None,
        },
        [command, _, _, _] if command == "dsc-sidecar" => Command::DscSidecar {
            input: PathBuf::from(&args[1]),
            lossless_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        },
        _ => return None,
    };
    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_optional_lossless_and_observation_outputs() {
        let args: Vec<OsString> = ["dsc-sidecar", "in.dsc", "copy.dsc", "result.obs"]
            .into_iter()
            .map(OsString::from)
            .collect();
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args, &text),
            Some(Command::DscSidecar {
                lossless_output: Some(_),
                observation: Some(_),
                ..
            })
        ));
    }
}
