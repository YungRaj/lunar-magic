use crate::{
    arg_values::ArgsError,
    command_types::{Command, NativeMap16SidecarKind},
};
use std::{borrow::Cow, ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[Cow<'_, str>]) -> Result<Option<Command>, ArgsError> {
    let make = |kind: &str, normalized: Option<usize>, observation: Option<usize>| {
        Ok(Some(Command::NativeMap16Sidecar {
            kind: parse_kind(kind)?,
            input: PathBuf::from(&args[2]),
            normalized_output: normalized.map(|index| PathBuf::from(&args[index])),
            observation: observation.map(|index| PathBuf::from(&args[index])),
        }))
    };
    match text {
        [command, _] if command == "lm16-map16-file" => Ok(Some(Command::Lm16Map16File {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
        })),
        [command, _, _] if command == "lm16-map16-file" => Ok(Some(Command::Lm16Map16File {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
        })),
        [command, kind, _] if command == "native-map16-sidecar" => make(kind, None, None),
        [command, kind, _, _] if command == "native-map16-sidecar" => make(kind, Some(3), None),
        [command, kind, _, _, _] if command == "native-map16-sidecar" => {
            make(kind, Some(3), Some(4))
        }
        _ => Ok(None),
    }
}

fn parse_kind(value: &str) -> Result<NativeMap16SidecarKind, ArgsError> {
    match value {
        "m16" => Ok(NativeMap16SidecarKind::M16),
        "s16" => Ok(NativeMap16SidecarKind::S16),
        _ => Err(ArgsError(format!(
            "unknown native Map16 sidecar kind {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_kinds_and_optional_outputs() {
        let container: Vec<OsString> = ["lm16-map16-file", "all.map16", "normalized.map16"]
            .into_iter()
            .map(OsString::from)
            .collect();
        let container_text: Vec<_> = container
            .iter()
            .map(|value| value.to_string_lossy())
            .collect();
        assert!(matches!(
            parse(&container, &container_text).unwrap(),
            Some(Command::Lm16Map16File {
                normalized_output: Some(_),
                ..
            })
        ));
        let args: Vec<OsString> = ["native-map16-sidecar", "s16", "in.s16", "out.s16", "x.obs"]
            .into_iter()
            .map(OsString::from)
            .collect();
        let text: Vec<_> = args.iter().map(|value| value.to_string_lossy()).collect();
        assert!(matches!(
            parse(&args, &text).unwrap(),
            Some(Command::NativeMap16Sidecar {
                kind: NativeMap16SidecarKind::S16,
                observation: Some(_),
                ..
            })
        ));
        let bad: Vec<_> = ["native-map16-sidecar", "bad", "in"]
            .into_iter()
            .map(OsString::from)
            .collect();
        let bad_text: Vec<_> = bad.iter().map(|value| value.to_string_lossy()).collect();
        assert!(parse(&bad, &bad_text).is_err());
    }
}
