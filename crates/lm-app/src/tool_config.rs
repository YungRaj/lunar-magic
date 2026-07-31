//! Deterministic portable persistence for external-tool configuration.

use std::path::PathBuf;

use crate::{ExternalTool, ExternalToolError, ToolEvent, validate_tools};

const MAGIC: &[u8; 8] = b"LMTOOLS1";
const MAX_TOOLS: usize = 256;
const MAX_ARGUMENTS: usize = 256;
const MAX_STRING_BYTES: usize = 1 << 20;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolConfig {
    pub tools: Vec<ExternalTool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolConfigError {
    EncodedTooLong(usize),
    WrongMagic,
    Truncated,
    TrailingBytes,
    InvalidUtf8,
    NonUtf8Path,
    TooManyTools(usize),
    TooManyArguments(usize),
    StringTooLong(usize),
    InvalidEventBits(u8),
    InvalidWorkingDirectoryFlag(u8),
    InvalidTool(ExternalToolError),
    Overflow,
}

impl std::fmt::Display for ToolConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "external-tool configuration error: {self:?}")
    }
}

impl std::error::Error for ToolConfigError {}

impl From<ExternalToolError> for ToolConfigError {
    fn from(value: ExternalToolError) -> Self {
        Self::InvalidTool(value)
    }
}

impl ToolConfig {
    /// Maximum portable configuration size accepted by the application.
    pub const MAX_ENCODED_LEN: usize = 64 * 1024 * 1024;

    /// Serializes this configuration into the versioned `LMTOOLS1` interchange format.
    ///
    /// # Errors
    ///
    /// Returns [`ToolConfigError`] for invalid tools, platform paths not representable as UTF-8,
    /// excessive counts or strings, and size overflow.
    pub fn encode(&self) -> Result<Vec<u8>, ToolConfigError> {
        validate_tools(&self.tools)?;
        if self.tools.len() > MAX_TOOLS {
            return Err(ToolConfigError::TooManyTools(self.tools.len()));
        }
        let encoded_len = self.encoded_len_with_limit(Self::MAX_ENCODED_LEN)?;
        let mut output = Vec::with_capacity(encoded_len);
        output.extend_from_slice(MAGIC);
        write_u16(&mut output, self.tools.len())?;
        for tool in &self.tools {
            write_string(&mut output, &tool.id)?;
            write_string(&mut output, &tool.name)?;
            write_string(
                &mut output,
                tool.executable
                    .to_str()
                    .ok_or(ToolConfigError::NonUtf8Path)?,
            )?;
            if tool.arguments.len() > MAX_ARGUMENTS {
                return Err(ToolConfigError::TooManyArguments(tool.arguments.len()));
            }
            write_u16(&mut output, tool.arguments.len())?;
            for argument in &tool.arguments {
                write_string(&mut output, argument)?;
            }
            match &tool.working_directory {
                Some(directory) => {
                    output.push(1);
                    write_string(&mut output, directory)?;
                }
                None => output.push(0),
            }
            let event_bits = tool.subscriptions.iter().fold(0, |bits, event| {
                bits | match event {
                    ToolEvent::ProjectOpened => 1,
                    ToolEvent::ProjectSaved => 2,
                    ToolEvent::LevelChanged => 4,
                }
            });
            output.push(event_bits);
        }
        if output.len() > Self::MAX_ENCODED_LEN {
            return Err(ToolConfigError::EncodedTooLong(output.len()));
        }
        Ok(output)
    }

    fn encoded_len_with_limit(&self, limit: usize) -> Result<usize, ToolConfigError> {
        let mut len = MAGIC
            .len()
            .checked_add(2)
            .ok_or(ToolConfigError::Overflow)?;
        for tool in &self.tools {
            len = checked_string_len(len, &tool.id)?;
            len = checked_string_len(len, &tool.name)?;
            let executable = tool
                .executable
                .to_str()
                .ok_or(ToolConfigError::NonUtf8Path)?;
            len = checked_string_len(len, executable)?;
            if tool.arguments.len() > MAX_ARGUMENTS {
                return Err(ToolConfigError::TooManyArguments(tool.arguments.len()));
            }
            len = len.checked_add(2).ok_or(ToolConfigError::Overflow)?;
            for argument in &tool.arguments {
                len = checked_string_len(len, argument)?;
            }
            len = len.checked_add(1).ok_or(ToolConfigError::Overflow)?;
            if let Some(directory) = &tool.working_directory {
                len = checked_string_len(len, directory)?;
            }
            len = len.checked_add(1).ok_or(ToolConfigError::Overflow)?;
            if len > limit {
                return Err(ToolConfigError::EncodedTooLong(len));
            }
        }
        if len > limit {
            return Err(ToolConfigError::EncodedTooLong(len));
        }
        Ok(len)
    }

    /// Parses a complete bounded `LMTOOLS1` configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ToolConfigError`] for malformed framing, invalid UTF-8, unknown flags, invalid
    /// tools, trailing bytes, or configured limits being exceeded.
    pub fn decode(bytes: &[u8]) -> Result<Self, ToolConfigError> {
        if bytes.len() > Self::MAX_ENCODED_LEN {
            return Err(ToolConfigError::EncodedTooLong(bytes.len()));
        }
        let mut reader = Reader::new(bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(ToolConfigError::WrongMagic);
        }
        let tool_count = usize::from(reader.u16()?);
        if tool_count > MAX_TOOLS {
            return Err(ToolConfigError::TooManyTools(tool_count));
        }
        let mut tools = Vec::with_capacity(tool_count);
        for _ in 0..tool_count {
            let id = reader.string()?;
            let name = reader.string()?;
            let executable = PathBuf::from(reader.string()?);
            let argument_count = usize::from(reader.u16()?);
            if argument_count > MAX_ARGUMENTS {
                return Err(ToolConfigError::TooManyArguments(argument_count));
            }
            let mut arguments = Vec::with_capacity(argument_count);
            for _ in 0..argument_count {
                arguments.push(reader.string()?);
            }
            let working_directory = match reader.byte()? {
                0 => None,
                1 => Some(reader.string()?),
                value => return Err(ToolConfigError::InvalidWorkingDirectoryFlag(value)),
            };
            let event_bits = reader.byte()?;
            if event_bits & !7 != 0 {
                return Err(ToolConfigError::InvalidEventBits(event_bits));
            }
            let mut subscriptions = Vec::new();
            for (bit, event) in [
                (1, ToolEvent::ProjectOpened),
                (2, ToolEvent::ProjectSaved),
                (4, ToolEvent::LevelChanged),
            ] {
                if event_bits & bit != 0 {
                    subscriptions.push(event);
                }
            }
            tools.push(ExternalTool {
                id,
                name,
                executable,
                arguments,
                working_directory,
                subscriptions,
            });
        }
        if !reader.is_empty() {
            return Err(ToolConfigError::TrailingBytes);
        }
        validate_tools(&tools)?;
        Ok(Self { tools })
    }
}

fn write_u16(output: &mut Vec<u8>, value: usize) -> Result<(), ToolConfigError> {
    let value = u16::try_from(value).map_err(|_| ToolConfigError::Overflow)?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_string(output: &mut Vec<u8>, value: &str) -> Result<(), ToolConfigError> {
    if value.len() > MAX_STRING_BYTES {
        return Err(ToolConfigError::StringTooLong(value.len()));
    }
    let len = u32::try_from(value.len()).map_err(|_| ToolConfigError::Overflow)?;
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn checked_string_len(current: usize, value: &str) -> Result<usize, ToolConfigError> {
    if value.len() > MAX_STRING_BYTES {
        return Err(ToolConfigError::StringTooLong(value.len()));
    }
    current
        .checked_add(4)
        .and_then(|len| len.checked_add(value.len()))
        .ok_or(ToolConfigError::Overflow)
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ToolConfigError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(ToolConfigError::Overflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ToolConfigError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, ToolConfigError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ToolConfigError> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("slice length checked");
        Ok(u16::from_le_bytes(bytes))
    }

    fn string(&mut self) -> Result<String, ToolConfigError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("slice length checked");
        let len =
            usize::try_from(u32::from_le_bytes(bytes)).map_err(|_| ToolConfigError::Overflow)?;
        if len > MAX_STRING_BYTES {
            return Err(ToolConfigError::StringTooLong(len));
        }
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(|_| ToolConfigError::InvalidUtf8)
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ToolConfig {
        ToolConfig {
            tools: vec![ExternalTool {
                id: "emu".into(),
                name: "Émulateur".into(),
                executable: PathBuf::from("/Applications/Emu App"),
                arguments: vec!["--rom".into(), "{rom}".into()],
                working_directory: Some("{project_dir}".into()),
                subscriptions: vec![ToolEvent::ProjectSaved, ToolEvent::LevelChanged],
            }],
        }
    }

    #[test]
    fn round_trips_deterministically() {
        let expected = config();
        let bytes = expected.encode().unwrap();
        assert_eq!(ToolConfig::decode(&bytes).unwrap(), expected);
        assert_eq!(ToolConfig::decode(&bytes).unwrap().encode().unwrap(), bytes);
    }

    #[test]
    fn graphics_editor_template_round_trips_without_a_schema_change() {
        let mut expected = config();
        expected.tools[0].arguments.push("--gfx={graphics}".into());
        let decoded = ToolConfig::decode(&expected.encode().unwrap()).unwrap();
        assert_eq!(decoded, expected);
        assert!(decoded.tools[0].uses_argument_placeholder("graphics"));
    }

    #[test]
    fn encoder_preflights_the_complete_aggregate_before_allocating() {
        let config = config();
        let exact = config.encoded_len_with_limit(usize::MAX).unwrap();
        assert_eq!(config.encoded_len_with_limit(exact).unwrap(), exact);
        assert_eq!(
            config.encoded_len_with_limit(exact - 1),
            Err(ToolConfigError::EncodedTooLong(exact))
        );
        assert_eq!(config.encode().unwrap().len(), exact);
    }

    #[test]
    fn rejects_truncation_trailing_bytes_and_unknown_flags() {
        let bytes = config().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(ToolConfig::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            ToolConfig::decode(&trailing),
            Err(ToolConfigError::TrailingBytes)
        );

        let mut unknown_event = bytes;
        *unknown_event.last_mut().unwrap() = 0x80;
        assert_eq!(
            ToolConfig::decode(&unknown_event),
            Err(ToolConfigError::InvalidEventBits(0x80))
        );
    }

    #[test]
    fn rejects_noncanonical_duplicate_event_subscriptions_before_encoding() {
        let mut duplicate = config();
        duplicate.tools[0]
            .subscriptions
            .push(ToolEvent::ProjectSaved);
        assert_eq!(
            duplicate.encode(),
            Err(ToolConfigError::InvalidTool(
                ExternalToolError::DuplicateSubscription {
                    tool_id: "emu".into(),
                    event: ToolEvent::ProjectSaved,
                }
            ))
        );
    }
}
