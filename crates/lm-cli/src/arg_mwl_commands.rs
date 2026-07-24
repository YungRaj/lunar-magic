use crate::arg_values::{ArgsError, parse_number};
use crate::command_types::Command;
use std::borrow::Cow;
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn parse_mwl_command(
    args: &[OsString],
    text: &[Cow<'_, str>],
) -> Result<Option<Command>, ArgsError> {
    Ok(match text {
        [command, _] if command == "mwl" => Some(Command::Mwl(PathBuf::from(&args[1]))),
        [command, _] if command == "mwl-corpus" => Some(Command::MwlCorpus {
            root: PathBuf::from(&args[1]),
        }),
        [command, _, _] if command == "mwl-normalize" => Some(Command::MwlNormalize {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
        }),
        [command, _, _] if command == "mwl-observe" => Some(Command::MwlObserve {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
        }),
        [command, _, _, _, _] if command == "mwl-observe-optional-assets" => {
            Some(Command::MwlObserveOptionalAssets {
                input: PathBuf::from(&args[1]),
                size_modes: PathBuf::from(&args[2]),
                maximum_records: usize::try_from(parse_number(&text[3])?)
                    .map_err(|_| ArgsError("maximum record count does not fit usize".into()))?,
                output: PathBuf::from(&args[4]),
            })
        }
        [command, _, _] if command == "mwl-palette-tpl" => Some(Command::MwlPaletteTpl {
            input: PathBuf::from(&args[1]),
            output: PathBuf::from(&args[2]),
        }),
        [command, _, _, _, _, _] if command == "mwl-transfer-optional-assets" => {
            Some(Command::MwlTransferOptionalAssets {
                source: PathBuf::from(&args[1]),
                target: PathBuf::from(&args[2]),
                size_modes: PathBuf::from(&args[3]),
                maximum_records: usize::try_from(parse_number(&text[4])?)
                    .map_err(|_| ArgsError("maximum record count does not fit usize".into()))?,
                output: PathBuf::from(&args[5]),
            })
        }
        [command, _, _, _, _, _] if command == "mwl-edit-optional-assets" => {
            Some(Command::MwlEditOptionalAssets {
                input: PathBuf::from(&args[1]),
                size_modes: PathBuf::from(&args[2]),
                maximum_records: usize::try_from(parse_number(&text[3])?)
                    .map_err(|_| ArgsError("maximum record count does not fit usize".into()))?,
                edits: PathBuf::from(&args[4]),
                output: PathBuf::from(&args[5]),
            })
        }
        [command, _, enabled, file, length, offset, _] if command == "mwl-edit-layer3-settings" => {
            Some(Command::MwlEditLayer3Settings {
                input: PathBuf::from(&args[1]),
                enabled: match enabled.as_ref() {
                    "on" => true,
                    "off" => false,
                    _ => return Err(ArgsError("Layer 3 enabled state must be on or off".into())),
                },
                file: u16::try_from(parse_number(file)?)
                    .map_err(|_| ArgsError("Layer 3 file does not fit u16".into()))?,
                length_selector: u8::try_from(parse_number(length)?)
                    .map_err(|_| ArgsError("Layer 3 length selector does not fit u8".into()))?,
                offset_selector: u8::try_from(parse_number(offset)?)
                    .map_err(|_| ArgsError("Layer 3 offset selector does not fit u8".into()))?,
                output: PathBuf::from(&args[6]),
            })
        }
        [command, _, _] if command == "mwl-observe-layer3-settings" => {
            Some(Command::MwlObserveLayer3Settings {
                input: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        _ => None,
    })
}
