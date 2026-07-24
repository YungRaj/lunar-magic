use lm_app::{ExternalToolError, FrontendEffect, ToolEvent, ToolInvocation};
use std::fmt;
use std::io;
use std::process::Command;

#[derive(Debug)]
pub enum ToolLaunchError {
    Resolve {
        tool_id: String,
        source: ExternalToolError,
    },
    Start {
        tool_id: String,
        source: io::Error,
    },
    Exit {
        tool_id: String,
        code: Option<i32>,
    },
}

impl fmt::Display for ToolLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resolve { tool_id, source } => {
                write!(
                    formatter,
                    "could not resolve external tool {tool_id:?}: {source}"
                )
            }
            Self::Start { tool_id, source } => {
                write!(
                    formatter,
                    "could not start external tool {tool_id:?}: {source}"
                )
            }
            Self::Exit { tool_id, code } => match code {
                Some(code) => write!(
                    formatter,
                    "external tool {tool_id:?} exited with code {code}"
                ),
                None => write!(
                    formatter,
                    "external tool {tool_id:?} terminated without an exit code"
                ),
            },
        }
    }
}

impl std::error::Error for ToolLaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Resolve { source, .. } => Some(source),
            Self::Start { source, .. } => Some(source),
            Self::Exit { .. } => None,
        }
    }
}

pub fn parse_event(value: &str) -> Result<ToolEvent, &'static str> {
    match value {
        "opened" => Ok(ToolEvent::ProjectOpened),
        "saved" => Ok(ToolEvent::ProjectSaved),
        "level" => Ok(ToolEvent::LevelChanged),
        _ => Err("tool event must be opened, saved, or level"),
    }
}

pub fn invocation_lines(invocation: &ToolInvocation) -> Vec<String> {
    let mut lines = vec![
        format!("tool invocation {:?}", invocation.tool_id),
        format!("  executable: {}", invocation.executable.display()),
        format!(
            "  working directory: {}",
            invocation
                .working_directory
                .as_deref()
                .map_or_else(|| "<inherited>".into(), |path| path.display().to_string())
        ),
    ];
    lines.extend(
        invocation
            .arguments
            .iter()
            .enumerate()
            .map(|(index, argument)| format!("  argument[{index}]: {argument:?}")),
    );
    lines
}

pub fn print_invocations(effects: &[FrontendEffect]) {
    let mut count = 0;
    for invocation in effects.iter().filter_map(|effect| match effect {
        FrontendEffect::LaunchExternalTool(invocation) => Some(invocation),
        _ => None,
    }) {
        count += 1;
        for line in invocation_lines(invocation) {
            println!("{line}");
        }
    }
    if count == 0 {
        println!("no external tools matched");
    }
    for effect in effects {
        if let FrontendEffect::ExternalToolFailed { tool_id, error } = effect {
            println!("tool {tool_id:?} could not be resolved: {error}");
        }
    }
}

pub fn print_event_invocations(effects: &[FrontendEffect]) {
    if effects.iter().any(|effect| {
        matches!(
            effect,
            FrontendEffect::LaunchExternalTool(_) | FrontendEffect::ExternalToolFailed { .. }
        )
    }) {
        print_invocations(effects);
    }
}

/// Executes resolved tool invocations directly, never through a command shell.
///
/// Tools run sequentially and the first launch or non-success exit stops the operation. This is
/// deliberately separate from previewing an invocation so a frontend must opt in explicitly.
pub fn execute_invocations(effects: &[FrontendEffect]) -> Result<usize, ToolLaunchError> {
    if let Some((tool_id, source)) = effects.iter().find_map(|effect| match effect {
        FrontendEffect::ExternalToolFailed { tool_id, error } => {
            Some((tool_id.clone(), error.clone()))
        }
        _ => None,
    }) {
        return Err(ToolLaunchError::Resolve { tool_id, source });
    }
    let invocations = effects.iter().filter_map(|effect| match effect {
        FrontendEffect::LaunchExternalTool(invocation) => Some(invocation),
        _ => None,
    });
    let mut count = 0;
    for invocation in invocations {
        count += 1;
        let status =
            process_command(invocation)
                .status()
                .map_err(|source| ToolLaunchError::Start {
                    tool_id: invocation.tool_id.clone(),
                    source,
                })?;
        if !status.success() {
            return Err(ToolLaunchError::Exit {
                tool_id: invocation.tool_id.clone(),
                code: status.code(),
            });
        }
        println!(
            "external tool {:?} completed successfully",
            invocation.tool_id
        );
    }
    if count == 0 {
        println!("no external tools matched");
    }
    Ok(count)
}

fn process_command(invocation: &ToolInvocation) -> Command {
    let mut command = Command::new(&invocation.executable);
    command.args(&invocation.arguments);
    if let Some(directory) = &invocation.working_directory {
        command.current_dir(directory);
    }
    command
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn parses_only_named_events() {
        assert_eq!(parse_event("opened"), Ok(ToolEvent::ProjectOpened));
        assert_eq!(parse_event("saved"), Ok(ToolEvent::ProjectSaved));
        assert_eq!(parse_event("level"), Ok(ToolEvent::LevelChanged));
        assert!(parse_event("save").is_err());
    }

    #[test]
    fn formats_arguments_independently_without_constructing_a_shell_command() {
        let lines = invocation_lines(&ToolInvocation {
            tool_id: "emu".into(),
            executable: PathBuf::from("/Applications/Émulateur App"),
            arguments: vec!["--rom".into(), "/tmp/My Hack 日本語.smc".into()],
            working_directory: Some(PathBuf::from("/tmp/My Project")),
        });
        assert_eq!(lines[3], "  argument[0]: \"--rom\"");
        assert_eq!(lines[4], "  argument[1]: \"/tmp/My Hack 日本語.smc\"");
    }

    #[test]
    fn constructs_a_direct_process_with_independent_unicode_arguments() {
        let invocation = ToolInvocation {
            tool_id: "emu".into(),
            executable: PathBuf::from("Emulator App"),
            arguments: vec!["--rom".into(), "My Hack 日本語.smc".into()],
            working_directory: Some(PathBuf::from("Project Folder")),
        };
        let command = process_command(&invocation);
        assert_eq!(command.get_program(), "Emulator App");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--rom", "My Hack 日本語.smc"]
        );
        assert_eq!(
            command.get_current_dir(),
            Some(PathBuf::from("Project Folder").as_path())
        );
    }

    #[test]
    fn reports_a_failed_direct_process_start_with_the_tool_identity() {
        let effects = [FrontendEffect::LaunchExternalTool(ToolInvocation {
            tool_id: "missing-emulator".into(),
            executable: PathBuf::from("path-that-cannot-name-a-real-lm-test-program"),
            arguments: Vec::new(),
            working_directory: None,
        })];
        let error = execute_invocations(&effects).unwrap_err();
        assert!(matches!(
            error,
            ToolLaunchError::Start { tool_id, .. } if tool_id == "missing-emulator"
        ));
    }

    #[test]
    fn refuses_to_treat_a_resolution_failure_as_an_empty_event() {
        let effects = [FrontendEffect::ExternalToolFailed {
            tool_id: "emu".into(),
            error: ExternalToolError::MissingValue("rom"),
        }];
        assert!(matches!(
            execute_invocations(&effects),
            Err(ToolLaunchError::Resolve { tool_id, source: ExternalToolError::MissingValue("rom") })
                if tool_id == "emu"
        ));
    }
}
