//! Portable toolbar layout and action identifiers for native frontends.

use std::collections::BTreeSet;

use crate::{Command, LevelNavigationDirection, UiTextKey};

const MAGIC: &[u8; 8] = b"LMTBAR01";
const MAX_ITEMS: usize = 128;
const MAX_ID_BYTES: usize = 64;
const MAX_ENCODED_BYTES: usize = MAGIC.len() + 2 + MAX_ITEMS * (4 + MAX_ID_BYTES);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ToolbarAction {
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    ShowOverworld,
    ShowMap16,
    LevelBack,
    LevelForward,
}

impl ToolbarAction {
    const ALL: [Self; 12] = [
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::Undo,
        Self::Redo,
        Self::Copy,
        Self::Cut,
        Self::Paste,
        Self::ShowOverworld,
        Self::ShowMap16,
        Self::LevelBack,
        Self::LevelForward,
    ];

    pub(crate) fn from_byte(value: u8) -> Option<Self> {
        Self::ALL.get(usize::from(value)).copied()
    }

    #[must_use]
    pub fn activation(self) -> ToolbarActivation {
        match self {
            Self::Open => ToolbarActivation::command(Command::Open),
            Self::Save => ToolbarActivation::command(Command::Save),
            Self::SaveAs => ToolbarActivation::command(Command::SaveAs),
            Self::Undo => ToolbarActivation::command(Command::Undo),
            Self::Redo => ToolbarActivation::command(Command::Redo),
            Self::Copy => ToolbarActivation::RequestCopyPayload,
            Self::Cut => ToolbarActivation::RequestCutPayload,
            Self::Paste => ToolbarActivation::RequestClipboardBytes,
            Self::ShowOverworld => ToolbarActivation::command(Command::ShowOverworld),
            Self::ShowMap16 => ToolbarActivation::command(Command::ShowMap16),
            Self::LevelBack => {
                ToolbarActivation::command(Command::NavigateLevel(LevelNavigationDirection::Back))
            }
            Self::LevelForward => ToolbarActivation::command(Command::NavigateLevel(
                LevelNavigationDirection::Forward,
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolbarActivation {
    Command(Box<Command>),
    /// The active editor must serialize the current selection before dispatching [`Command::Copy`].
    RequestCopyPayload,
    /// The active editor must serialize the current selection before dispatching [`Command::Cut`].
    RequestCutPayload,
    /// The frontend must read the application clipboard MIME type before dispatching paste.
    RequestClipboardBytes,
}

impl ToolbarActivation {
    #[must_use]
    pub fn command(command: Command) -> Self {
        Self::Command(Box::new(command))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolbarItem {
    Action {
        id: String,
        action: ToolbarAction,
        label: UiTextKey,
    },
    Separator,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolbarConfig {
    pub items: Vec<ToolbarItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolbarError {
    WrongMagic,
    Truncated,
    TrailingBytes,
    InvalidUtf8,
    TooManyItems(usize),
    Empty,
    EdgeSeparator,
    ConsecutiveSeparators,
    InvalidId(String),
    DuplicateId(String),
    UnknownItem(u8),
    UnknownAction(u8),
    UnknownLabel(u8),
    Overflow,
}

impl std::fmt::Display for ToolbarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "toolbar configuration error: {self:?}")
    }
}

impl std::error::Error for ToolbarError {}

impl ToolbarConfig {
    /// Maximum canonical `LMTBAR01` size accepted by native bounded loaders.
    pub const MAX_ENCODED_LEN: usize = MAX_ENCODED_BYTES;

    /// Validates item limits, stable identifiers, and separator placement.
    ///
    /// # Errors
    ///
    /// Returns [`ToolbarError`] for empty, excessive, ambiguous, or malformed layouts.
    pub fn validate(&self) -> Result<(), ToolbarError> {
        if self.items.is_empty() {
            return Err(ToolbarError::Empty);
        }
        if self.items.len() > MAX_ITEMS {
            return Err(ToolbarError::TooManyItems(self.items.len()));
        }
        if matches!(self.items.first(), Some(ToolbarItem::Separator))
            || matches!(self.items.last(), Some(ToolbarItem::Separator))
        {
            return Err(ToolbarError::EdgeSeparator);
        }
        let mut ids = BTreeSet::new();
        let mut separator = false;
        for item in &self.items {
            match item {
                ToolbarItem::Separator => {
                    if separator {
                        return Err(ToolbarError::ConsecutiveSeparators);
                    }
                    separator = true;
                }
                ToolbarItem::Action { id, .. } => {
                    separator = false;
                    if !valid_id(id) {
                        return Err(ToolbarError::InvalidId(id.clone()));
                    }
                    if !ids.insert(id) {
                        return Err(ToolbarError::DuplicateId(id.clone()));
                    }
                }
            }
        }
        Ok(())
    }

    /// Encodes this layout canonically as `LMTBAR01`.
    ///
    /// # Errors
    ///
    /// Returns [`ToolbarError`] if the layout is invalid or cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, ToolbarError> {
        self.validate()?;
        let mut output = MAGIC.to_vec();
        let count = u16::try_from(self.items.len()).map_err(|_| ToolbarError::Overflow)?;
        output.extend_from_slice(&count.to_le_bytes());
        for item in &self.items {
            match item {
                ToolbarItem::Separator => output.push(0),
                ToolbarItem::Action { id, action, label } => {
                    output.push(1);
                    output.push(*action as u8);
                    output.push(*label as u8);
                    output.push(u8::try_from(id.len()).map_err(|_| ToolbarError::Overflow)?);
                    output.extend_from_slice(id.as_bytes());
                }
            }
        }
        Ok(output)
    }

    /// Decodes one complete bounded `LMTBAR01` layout.
    ///
    /// # Errors
    ///
    /// Returns [`ToolbarError`] for malformed framing, unknown actions/labels, invalid UTF-8,
    /// invalid structure, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ToolbarError> {
        let mut reader = Reader::new(bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(ToolbarError::WrongMagic);
        }
        let count = usize::from(reader.u16()?);
        if count > MAX_ITEMS {
            return Err(ToolbarError::TooManyItems(count));
        }
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            match reader.byte()? {
                0 => items.push(ToolbarItem::Separator),
                1 => {
                    let raw_action = reader.byte()?;
                    let action = ToolbarAction::from_byte(raw_action)
                        .ok_or(ToolbarError::UnknownAction(raw_action))?;
                    let raw_label = reader.byte()?;
                    let label = UiTextKey::ALL
                        .get(usize::from(raw_label))
                        .copied()
                        .ok_or(ToolbarError::UnknownLabel(raw_label))?;
                    let id = reader.string()?;
                    items.push(ToolbarItem::Action { id, action, label });
                }
                value => return Err(ToolbarError::UnknownItem(value)),
            }
        }
        if !reader.is_empty() {
            return Err(ToolbarError::TrailingBytes);
        }
        let config = Self { items };
        config.validate()?;
        Ok(config)
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], ToolbarError> {
        let end = self.offset.checked_add(len).ok_or(ToolbarError::Overflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ToolbarError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, ToolbarError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ToolbarError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("length checked"),
        ))
    }

    fn string(&mut self) -> Result<String, ToolbarError> {
        let len = usize::from(self.byte()?);
        if len > MAX_ID_BYTES {
            return Err(ToolbarError::Overflow);
        }
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(|_| ToolbarError::InvalidUtf8)
    }

    const fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ToolbarConfig {
        ToolbarConfig {
            items: vec![
                ToolbarItem::Action {
                    id: "file.open".into(),
                    action: ToolbarAction::Open,
                    label: UiTextKey::FileOpen,
                },
                ToolbarItem::Separator,
                ToolbarItem::Action {
                    id: "edit.undo".into(),
                    action: ToolbarAction::Undo,
                    label: UiTextKey::EditUndo,
                },
            ],
        }
    }

    #[test]
    fn portable_layout_round_trips_canonically() {
        let expected = config();
        let bytes = expected.encode().unwrap();
        assert_eq!(ToolbarConfig::decode(&bytes).unwrap(), expected);
        assert_eq!(
            ToolbarConfig::decode(&bytes).unwrap().encode().unwrap(),
            bytes
        );
    }

    #[test]
    fn appended_level_navigation_actions_round_trip_without_renumbering_legacy_actions() {
        assert_eq!(ToolbarAction::ShowMap16 as u8, 9);
        let expected = ToolbarConfig {
            items: vec![
                ToolbarItem::Action {
                    id: "level.back".into(),
                    action: ToolbarAction::LevelBack,
                    label: UiTextKey::EditUndo,
                },
                ToolbarItem::Action {
                    id: "level.forward".into(),
                    action: ToolbarAction::LevelForward,
                    label: UiTextKey::EditRedo,
                },
            ],
        };
        assert_eq!(
            ToolbarConfig::decode(&expected.encode().unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn truncation_trailing_and_unknown_discriminants_are_rejected() {
        let bytes = config().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(ToolbarConfig::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            ToolbarConfig::decode(&trailing),
            Err(ToolbarError::TrailingBytes)
        );
        let mut unknown = bytes;
        unknown[MAGIC.len() + 2] = 9;
        assert_eq!(
            ToolbarConfig::decode(&unknown),
            Err(ToolbarError::UnknownItem(9))
        );
    }

    #[test]
    fn malformed_layouts_and_duplicate_ids_are_rejected() {
        for items in [
            vec![ToolbarItem::Separator],
            vec![
                ToolbarItem::Action {
                    id: "same".into(),
                    action: ToolbarAction::Open,
                    label: UiTextKey::FileOpen,
                },
                ToolbarItem::Action {
                    id: "same".into(),
                    action: ToolbarAction::Save,
                    label: UiTextKey::FileSave,
                },
            ],
        ] {
            assert!(ToolbarConfig { items }.validate().is_err());
        }
    }
}
