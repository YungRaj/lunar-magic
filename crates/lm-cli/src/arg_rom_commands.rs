use crate::arg_values::{parse_mapper, parse_number};
use crate::command_types::Command;
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn parse_rom_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    Ok(match text {
        [command, _, _, mapper, target, fill] if command == "rom-expand" => {
            Some(Command::RomExpand {
                input: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
                mapper: parse_mapper(mapper)?,
                target_logical_len: usize::try_from(parse_number(target)?)?,
                fill: u8::try_from(parse_number(fill)?)?,
            })
        }
        [command, _, _, fill] if command == "copier-header-add" => Some(Command::CopierHeaderAdd {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
            fill: u8::try_from(parse_number(fill)?)?,
        }),
        [command, _, _] if command == "copier-header-remove" => Some(Command::CopierHeaderRemove {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
        }),
        [command, _, _, _] if command == "ips-apply" => Some(Command::IpsApply {
            source: PathBuf::from(&args[1]),
            patch: PathBuf::from(&args[2]),
            output: PathBuf::from(&args[3]),
        }),
        [command, _, _, _] if command == "ips-create" => Some(Command::IpsCreate {
            before: PathBuf::from(&args[1]),
            after: PathBuf::from(&args[2]),
            output: PathBuf::from(&args[3]),
        }),
        _ => None,
    })
}

pub(crate) fn parse_rats_command(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    Ok(match text {
        [command, _] if command == "rats" => Some(Command::Rats(PathBuf::from(&args[1]))),
        [command, _, _] if command == "rats-observe" => Some(Command::RatsObserve {
            rom: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
        }),
        [command, _] if command == "rats-manifest" => Some(Command::RatsManifest {
            input: PathBuf::from(&args[1]),
            normalized_output: None,
            observation: None,
        }),
        [command, _, _] if command == "rats-manifest" => Some(Command::RatsManifest {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: None,
        }),
        [command, _, _, _] if command == "rats-manifest" => Some(Command::RatsManifest {
            input: PathBuf::from(&args[1]),
            normalized_output: Some(PathBuf::from(&args[2])),
            observation: Some(PathBuf::from(&args[3])),
        }),
        [command, _, _, fill] if command == "rats-plan" => Some(Command::RatsPlan {
            rom: PathBuf::from(&args[1]),
            manifest: PathBuf::from(&args[2]),
            fill: u8::try_from(parse_number(fill)?)?,
        }),
        [command, _, _, _, fill] if command == "rats-reclaim" => Some(Command::RatsReclaim {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
            manifest: PathBuf::from(&args[3]),
            fill: u8::try_from(parse_number(fill)?)?,
        }),
        _ => None,
    })
}
