//! Parsing for toolkit-neutral toolbar actions and logical shortcut gestures.

use lm_app::{ShortcutGesture, ShortcutKey, ShortcutModifiers, ToolbarAction};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiShellError {
    UnknownAction(String),
    EmptyGesture,
    DuplicateModifier(String),
    UnknownKey(String),
    MultipleKeys,
}

impl fmt::Display for UiShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid frontend action or shortcut: {self:?}")
    }
}

impl std::error::Error for UiShellError {}

pub fn parse_action(value: &str) -> Result<ToolbarAction, UiShellError> {
    match value {
        "open" => Ok(ToolbarAction::Open),
        "save" => Ok(ToolbarAction::Save),
        "save-as" => Ok(ToolbarAction::SaveAs),
        "undo" => Ok(ToolbarAction::Undo),
        "redo" => Ok(ToolbarAction::Redo),
        "copy" => Ok(ToolbarAction::Copy),
        "cut" => Ok(ToolbarAction::Cut),
        "paste" => Ok(ToolbarAction::Paste),
        "overworld" => Ok(ToolbarAction::ShowOverworld),
        "map16" => Ok(ToolbarAction::ShowMap16),
        "level-back" => Ok(ToolbarAction::LevelBack),
        "level-forward" => Ok(ToolbarAction::LevelForward),
        _ => Err(UiShellError::UnknownAction(value.into())),
    }
}

pub fn parse_gesture(value: &str) -> Result<ShortcutGesture, UiShellError> {
    if value.is_empty() {
        return Err(UiShellError::EmptyGesture);
    }
    let mut modifiers = ShortcutModifiers::default();
    let mut seen = BTreeSet::new();
    let mut key = None;
    for token in value.split('+') {
        if let Some(modifier) = parse_modifier(token) {
            if !seen.insert(token) {
                return Err(UiShellError::DuplicateModifier(token.into()));
            }
            modifiers = modifiers.union(modifier);
        } else if key.replace(parse_key(token)?).is_some() {
            return Err(UiShellError::MultipleKeys);
        }
    }
    Ok(ShortcutGesture {
        modifiers,
        key: key.ok_or(UiShellError::EmptyGesture)?,
    })
}

fn parse_modifier(value: &str) -> Option<ShortcutModifiers> {
    match value {
        "primary" => Some(ShortcutModifiers::PRIMARY),
        "secondary" => Some(ShortcutModifiers::SECONDARY),
        "shift" => Some(ShortcutModifiers::SHIFT),
        "alt" => Some(ShortcutModifiers::ALT),
        _ => None,
    }
}

fn parse_key(value: &str) -> Result<ShortcutKey, UiShellError> {
    match value {
        "backspace" => Ok(ShortcutKey::Backspace),
        "delete" => Ok(ShortcutKey::Delete),
        "enter" => Ok(ShortcutKey::Enter),
        "escape" => Ok(ShortcutKey::Escape),
        "left" => Ok(ShortcutKey::ArrowLeft),
        "right" => Ok(ShortcutKey::ArrowRight),
        "up" => Ok(ShortcutKey::ArrowUp),
        "down" => Ok(ShortcutKey::ArrowDown),
        _ => parse_variable_key(value),
    }
}

fn parse_variable_key(value: &str) -> Result<ShortcutKey, UiShellError> {
    if let Some(number) = value.strip_prefix('f') {
        return number
            .parse::<u8>()
            .ok()
            .filter(|number| (1..=24).contains(number))
            .map(ShortcutKey::Function)
            .ok_or_else(|| UiShellError::UnknownKey(value.into()));
    }
    let mut characters = value.chars();
    let character = characters
        .next()
        .filter(|character| characters.next().is_none() && !character.is_whitespace())
        .ok_or_else(|| UiShellError::UnknownKey(value.into()))?;
    Ok(ShortcutKey::Character(character))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_actions_and_portable_gestures() {
        assert_eq!(parse_action("map16").unwrap(), ToolbarAction::ShowMap16);
        assert_eq!(
            parse_action("level-back").unwrap(),
            ToolbarAction::LevelBack
        );
        assert_eq!(
            parse_action("level-forward").unwrap(),
            ToolbarAction::LevelForward
        );
        assert_eq!(
            parse_gesture("primary+shift+f12").unwrap(),
            ShortcutGesture {
                modifiers: ShortcutModifiers::PRIMARY.union(ShortcutModifiers::SHIFT),
                key: ShortcutKey::Function(12),
            }
        );
        assert_eq!(parse_gesture("⌘").unwrap().key, ShortcutKey::Character('⌘'));
    }

    #[test]
    fn rejects_unknown_duplicate_and_multiple_components() {
        assert!(parse_action("nope").is_err());
        assert!(parse_gesture("").is_err());
        assert!(parse_gesture("primary+primary+s").is_err());
        assert!(parse_gesture("primary+s+t").is_err());
        assert!(parse_gesture("f25").is_err());
    }
}
