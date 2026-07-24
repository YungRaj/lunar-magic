use crate::{
    arg_values::{ArgsError, parse_number},
    command_types::Command,
};
use std::{ffi::OsString, path::PathBuf};

pub fn parse(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    let (packed, observation) = match text {
        [command, packed, _, _, _] if command == "layer3-workspace-apply" => (packed, None),
        [command, packed, _, _, _, _] if command == "layer3-workspace-apply" => {
            (packed, Some(PathBuf::from(&args[5])))
        }
        _ => return Ok(None),
    };
    Ok(Some(Command::Layer3WorkspaceApply {
        packed_descriptor: u16::try_from(parse_number(packed)?)
            .map_err(|_| ArgsError("packed Layer 3 descriptor exceeds 16 bits".into()))?,
        workspace: PathBuf::from(&args[2]),
        decoded_graphics: PathBuf::from(&args[3]),
        output: PathBuf::from(&args[4]),
        observation,
    }))
}
