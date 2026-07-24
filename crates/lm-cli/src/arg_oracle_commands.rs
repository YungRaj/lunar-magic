use crate::arg_values::ArgsError;
use crate::command_types::{Command, OracleCaptureCommand, OracleOwnership};
use std::ffi::OsString;
use std::path::PathBuf;

pub(crate) fn parse_oracle_verification(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Option<Command> {
    match text {
        [command, _, _, _] if command == "oracle-verify" => Some(Command::OracleVerify {
            manifest: PathBuf::from(&args[1]),
            before: PathBuf::from(&args[2]),
            after: PathBuf::from(&args[3]),
            observations: None,
        }),
        [command, _, _, _, _, _] if command == "oracle-verify" => Some(Command::OracleVerify {
            manifest: PathBuf::from(&args[1]),
            before: PathBuf::from(&args[2]),
            after: PathBuf::from(&args[3]),
            observations: Some((PathBuf::from(&args[4]), PathBuf::from(&args[5]))),
        }),
        [command, _] if command == "oracle-verify-suite" => Some(Command::OracleVerifySuite {
            root: PathBuf::from(&args[1]),
        }),
        [command, _, requirements @ ..]
            if command == "oracle-coverage" && !requirements.is_empty() =>
        {
            Some(Command::OracleCoverage {
                root: PathBuf::from(&args[1]),
                requirements: requirements.iter().map(ToString::to_string).collect(),
            })
        }
        [command, _, requirements @ ..]
            if command == "oracle-release-gate" && !requirements.is_empty() =>
        {
            Some(Command::OracleReleaseGate {
                root: PathBuf::from(&args[1]),
                requirements: requirements.iter().map(ToString::to_string).collect(),
            })
        }
        _ => None,
    }
}

pub(crate) fn parse_oracle_capture(
    args: &[OsString],
    text: &[std::borrow::Cow<'_, str>],
) -> Result<Option<Command>, Box<dyn std::error::Error>> {
    if text
        .first()
        .is_none_or(|command| command != "oracle-capture")
    {
        return Ok(None);
    }
    if text.len() < 10 {
        return Err(ArgsError(
            "oracle-capture requires CASE VERSION OPERATION BEFORE AFTER DECODED_BEFORE DECODED_AFTER none|changed-rats OUTPUT [KEY=VALUE ...]".into(),
        )
        .into());
    }
    let ownership = match text[8].as_ref() {
        "none" => OracleOwnership::None,
        "changed-rats" => OracleOwnership::ChangedRats,
        value => return Err(ArgsError(format!("unknown oracle ownership policy {value}")).into()),
    };
    let mut arguments = Vec::new();
    for argument in &text[10..] {
        let (name, value) = argument
            .split_once('=')
            .ok_or_else(|| ArgsError(format!("oracle argument must be KEY=VALUE: {argument}")))?;
        if name.is_empty() {
            return Err(ArgsError("oracle argument name cannot be empty".into()).into());
        }
        arguments.push((name.into(), value.into()));
    }
    Ok(Some(Command::OracleCapture(OracleCaptureCommand {
        case_id: text[1].to_string(),
        lunar_magic_version: text[2].to_string(),
        operation: text[3].to_string(),
        before: PathBuf::from(&args[4]),
        after: PathBuf::from(&args[5]),
        decoded_before: PathBuf::from(&args[6]),
        decoded_after: PathBuf::from(&args[7]),
        ownership,
        output: PathBuf::from(&args[9]),
        arguments,
    })))
}
