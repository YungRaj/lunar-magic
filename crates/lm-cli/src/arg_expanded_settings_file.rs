use crate::command_types::Command;
use std::{ffi::OsString, path::PathBuf};

pub fn parse(args: &[OsString], text: &[std::borrow::Cow<'_, str>]) -> Option<Command> {
    match text {
        [command, _] if command == "expanded-settings-file" => {
            Some(Command::ExpandedSettingsFile {
                input: PathBuf::from(&args[1]),
                normalized_output: None,
                observation: None,
            })
        }
        [command, _, _] if command == "expanded-settings-file" => {
            Some(Command::ExpandedSettingsFile {
                input: PathBuf::from(&args[1]),
                normalized_output: Some(PathBuf::from(&args[2])),
                observation: None,
            })
        }
        [command, _, _, _] if command == "expanded-settings-file" => {
            Some(Command::ExpandedSettingsFile {
                input: PathBuf::from(&args[1]),
                normalized_output: Some(PathBuf::from(&args[2])),
                observation: Some(PathBuf::from(&args[3])),
            })
        }
        [command, _, enabled, file, length, offset, _] if command == "expanded-settings-layer3" => {
            Some(Command::ExpandedSettingsLayer3 {
                input: PathBuf::from(&args[1]),
                enabled: match enabled.as_ref() {
                    "on" => true,
                    "off" => false,
                    _ => return None,
                },
                file: hex(file)?,
                length_selector: hex(length)?,
                offset_selector: hex(offset)?,
                output: PathBuf::from(&args[6]),
            })
        }
        _ => None,
    }
}

fn hex<T: TryFrom<u64>>(value: &str) -> Option<T> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    u64::from_str_radix(value, 16)
        .ok()
        .and_then(|value| T::try_from(value).ok())
}
