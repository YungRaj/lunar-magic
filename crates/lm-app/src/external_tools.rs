//! Shell-free external-tool configuration and command-template expansion.

use std::path::{Path, PathBuf};

/// Application events to which an external tool may subscribe.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ToolEvent {
    ProjectOpened,
    ProjectSaved,
    LevelChanged,
}

/// A frontend-owned executable and its argument templates.
///
/// Arguments are expanded independently and are never interpreted by a shell. Consequently paths
/// containing whitespace, quotes, or non-ASCII characters remain one argument.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalTool {
    pub id: String,
    pub name: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    pub subscriptions: Vec<ToolEvent>,
}

/// Context made available to external-tool templates.
#[derive(Clone, Copy, Debug, Default)]
pub struct ToolContext<'a> {
    pub rom: Option<&'a Path>,
    pub level: Option<u16>,
}

/// An already expanded process request for a platform frontend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolInvocation {
    pub tool_id: String,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalToolError {
    EmptyId,
    EmptyName,
    EmptyExecutable,
    DuplicateId(String),
    DuplicateSubscription { tool_id: String, event: ToolEvent },
    UnknownTool(String),
    UnknownPlaceholder(String),
    MissingValue(&'static str),
    UnclosedPlaceholder,
    UnexpectedClosingBrace,
    NulByte,
    EmptyWorkingDirectory,
}

impl std::fmt::Display for ExternalToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "external-tool error: {self:?}")
    }
}

impl std::error::Error for ExternalToolError {}

impl ExternalTool {
    /// Validates and expands this configuration without spawning a process.
    ///
    /// Supported placeholders are `{rom}`, `{project_dir}`, `{level_hex}`, and `{level_dec}`.
    /// Literal braces are written as `{{` and `}}`.
    ///
    /// # Errors
    ///
    /// Returns [`ExternalToolError`] for invalid configuration, malformed templates, or values
    /// absent from `context`.
    pub fn expand(&self, context: ToolContext<'_>) -> Result<ToolInvocation, ExternalToolError> {
        self.validate()?;
        let arguments = self
            .arguments
            .iter()
            .map(|argument| expand_template(argument, context))
            .collect::<Result<Vec<_>, _>>()?;
        let working_directory = self
            .working_directory
            .as_deref()
            .map(|template| expand_template(template, context).map(PathBuf::from))
            .transpose()?;
        if working_directory
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(ExternalToolError::EmptyWorkingDirectory);
        }
        Ok(ToolInvocation {
            tool_id: self.id.clone(),
            executable: self.executable.clone(),
            arguments,
            working_directory,
        })
    }

    fn validate(&self) -> Result<(), ExternalToolError> {
        if self.id.is_empty() {
            return Err(ExternalToolError::EmptyId);
        }
        if self.name.is_empty() {
            return Err(ExternalToolError::EmptyName);
        }
        if self.executable.as_os_str().is_empty() {
            return Err(ExternalToolError::EmptyExecutable);
        }
        if self.id.contains('\0')
            || self.name.contains('\0')
            || self.arguments.iter().any(|value| value.contains('\0'))
            || self
                .working_directory
                .as_ref()
                .is_some_and(|value| value.contains('\0'))
        {
            return Err(ExternalToolError::NulByte);
        }
        Ok(())
    }
}

/// Validates a complete tool collection, including stable unique identifiers.
///
/// # Errors
///
/// Returns [`ExternalToolError`] when a tool is malformed or an identifier is duplicated.
pub fn validate_tools(tools: &[ExternalTool]) -> Result<(), ExternalToolError> {
    for (index, tool) in tools.iter().enumerate() {
        tool.validate()?;
        if tools[..index].iter().any(|other| other.id == tool.id) {
            return Err(ExternalToolError::DuplicateId(tool.id.clone()));
        }
        for (subscription, event) in tool.subscriptions.iter().enumerate() {
            if tool.subscriptions[..subscription].contains(event) {
                return Err(ExternalToolError::DuplicateSubscription {
                    tool_id: tool.id.clone(),
                    event: *event,
                });
            }
        }
    }
    Ok(())
}

fn expand_template(template: &str, context: ToolContext<'_>) -> Result<String, ExternalToolError> {
    if template.contains('\0') {
        return Err(ExternalToolError::NulByte);
    }
    let mut output = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                output.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                output.push('}');
            }
            '}' => return Err(ExternalToolError::UnexpectedClosingBrace),
            '{' => {
                let mut key = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => return Err(ExternalToolError::UnclosedPlaceholder),
                        Some(value) => key.push(value),
                    }
                }
                output.push_str(&placeholder(&key, context)?);
            }
            value => output.push(value),
        }
    }
    Ok(output)
}

fn placeholder(key: &str, context: ToolContext<'_>) -> Result<String, ExternalToolError> {
    match key {
        "rom" => context
            .rom
            .map(|path| path.to_string_lossy().into_owned())
            .ok_or(ExternalToolError::MissingValue("rom")),
        "project_dir" => context
            .rom
            .and_then(Path::parent)
            .map(|path| path.to_string_lossy().into_owned())
            .ok_or(ExternalToolError::MissingValue("project_dir")),
        "level_hex" => context
            .level
            .map(|level| format!("{level:03X}"))
            .ok_or(ExternalToolError::MissingValue("level_hex")),
        "level_dec" => context
            .level
            .map(|level| level.to_string())
            .ok_or(ExternalToolError::MissingValue("level_dec")),
        unknown => Err(ExternalToolError::UnknownPlaceholder(unknown.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(arguments: &[&str]) -> ExternalTool {
        ExternalTool {
            id: "emulator".into(),
            name: "Emulator".into(),
            executable: PathBuf::from("/Applications/Test Emulator"),
            arguments: arguments.iter().map(|value| (*value).into()).collect(),
            working_directory: Some("{project_dir}".into()),
            subscriptions: vec![ToolEvent::ProjectSaved],
        }
    }

    #[test]
    fn expands_each_argument_without_shell_tokenization() {
        let rom = Path::new("/tmp/Unicode Hacks/Kaizō World.smc");
        let invocation = tool(&["--rom", "{rom}", "--level={level_hex}", "{{literal}}"])
            .expand(ToolContext {
                rom: Some(rom),
                level: Some(0x105),
            })
            .unwrap();
        assert_eq!(
            invocation.arguments,
            [
                "--rom",
                "/tmp/Unicode Hacks/Kaizō World.smc",
                "--level=105",
                "{literal}"
            ]
        );
        assert_eq!(
            invocation.working_directory,
            Some(PathBuf::from("/tmp/Unicode Hacks"))
        );
    }

    #[test]
    fn rejects_missing_unknown_and_malformed_values() {
        assert_eq!(
            tool(&["{rom}"]).expand(ToolContext::default()),
            Err(ExternalToolError::MissingValue("rom"))
        );
        assert!(matches!(
            tool(&["{mystery}"]).expand(ToolContext::default()),
            Err(ExternalToolError::UnknownPlaceholder(value)) if value == "mystery"
        ));
        assert_eq!(
            tool(&["{rom"]).expand(ToolContext::default()),
            Err(ExternalToolError::UnclosedPlaceholder)
        );
        assert_eq!(
            tool(&["rom}"]).expand(ToolContext::default()),
            Err(ExternalToolError::UnexpectedClosingBrace)
        );
    }

    #[test]
    fn rejects_duplicate_ids() {
        assert_eq!(
            validate_tools(&[tool(&[]), tool(&[])]),
            Err(ExternalToolError::DuplicateId("emulator".into()))
        );
    }

    #[test]
    fn rejects_duplicate_subscriptions_that_cannot_round_trip_canonically() {
        let mut duplicate = tool(&[]);
        duplicate.subscriptions = vec![ToolEvent::ProjectSaved, ToolEvent::ProjectSaved];
        assert_eq!(
            validate_tools(&[duplicate]),
            Err(ExternalToolError::DuplicateSubscription {
                tool_id: "emulator".into(),
                event: ToolEvent::ProjectSaved,
            })
        );
    }
}
