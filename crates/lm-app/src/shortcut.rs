//! Portable logical keyboard shortcuts for native frontends.

use crate::ToolbarAction;
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"LMSHORT1";
const MAX_BINDINGS: usize = 256;
const MAX_ENCODED_BYTES: usize = MAGIC.len() + 2 + MAX_BINDINGS * 8;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ShortcutModifiers(u8);

impl ShortcutModifiers {
    pub const PRIMARY: Self = Self(1);
    pub const SHIFT: Self = Self(2);
    pub const ALT: Self = Self(4);
    pub const SECONDARY: Self = Self(8);
    const KNOWN: u8 = 0x0f;

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ShortcutKey {
    Character(char),
    Function(u8),
    Backspace,
    Delete,
    Enter,
    Escape,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
    Space,
    MouseLeft,
    MouseRight,
    MouseMiddle,
    MouseExtra1,
    MouseExtra2,
    Pause,
    NumpadMultiply,
    NumpadAdd,
    NumpadSeparator,
    NumpadSubtract,
    NumpadDecimal,
    NumpadDivide,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ShortcutGesture {
    pub modifiers: ShortcutModifiers,
    pub key: ShortcutKey,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShortcutBinding {
    pub gesture: ShortcutGesture,
    pub action: ToolbarAction,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShortcutConfig {
    pub bindings: Vec<ShortcutBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShortcutError {
    WrongMagic,
    Truncated,
    TrailingBytes(usize),
    TooManyBindings(usize),
    InvalidModifiers(u8),
    InvalidCharacter(u32),
    InvalidFunctionKey(u8),
    UnknownKey(u8),
    UnknownAction(u8),
    NonZeroReserved(u8),
    DuplicateGesture(ShortcutGesture),
    Overflow,
}

impl std::fmt::Display for ShortcutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "shortcut configuration error: {self:?}")
    }
}

impl std::error::Error for ShortcutError {}

impl ShortcutConfig {
    /// Maximum canonical `LMSHORT1` size accepted by native bounded loaders.
    pub const MAX_ENCODED_LEN: usize = MAX_ENCODED_BYTES;

    /// Validates binding limits, logical keys, modifiers, and gesture uniqueness.
    ///
    /// # Errors
    ///
    /// Returns [`ShortcutError`] for invalid or duplicate gestures.
    pub fn validate(&self) -> Result<(), ShortcutError> {
        if self.bindings.len() > MAX_BINDINGS {
            return Err(ShortcutError::TooManyBindings(self.bindings.len()));
        }
        let mut gestures = BTreeSet::new();
        for binding in &self.bindings {
            validate_gesture(binding.gesture)?;
            if !gestures.insert(binding.gesture) {
                return Err(ShortcutError::DuplicateGesture(binding.gesture));
            }
        }
        Ok(())
    }

    /// Resolves a logical gesture without depending on platform scan codes.
    #[must_use]
    pub fn action_for(&self, gesture: ShortcutGesture) -> Option<ToolbarAction> {
        self.bindings
            .iter()
            .find(|binding| binding.gesture == gesture)
            .map(|binding| binding.action)
    }

    /// Encodes bindings canonically in configured precedence order as `LMSHORT1`.
    ///
    /// # Errors
    ///
    /// Returns [`ShortcutError`] if the configuration is invalid or cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, ShortcutError> {
        self.validate()?;
        let mut output = MAGIC.to_vec();
        output.extend_from_slice(
            &u16::try_from(self.bindings.len())
                .map_err(|_| ShortcutError::Overflow)?
                .to_le_bytes(),
        );
        for binding in &self.bindings {
            output.push(binding.gesture.modifiers.0);
            let (kind, value) = encode_key(binding.gesture.key);
            output.push(kind);
            output.extend_from_slice(&value.to_le_bytes());
            output.push(binding.action as u8);
            output.push(0);
        }
        Ok(output)
    }

    /// Decodes one complete bounded `LMSHORT1` configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ShortcutError`] for malformed framing, invalid logical keys/modifiers, unknown
    /// actions, duplicate gestures, excessive counts, or trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, ShortcutError> {
        let header = bytes.get(..10).ok_or(ShortcutError::Truncated)?;
        if &header[..8] != MAGIC {
            return Err(ShortcutError::WrongMagic);
        }
        let count = usize::from(u16::from_le_bytes([header[8], header[9]]));
        if count > MAX_BINDINGS {
            return Err(ShortcutError::TooManyBindings(count));
        }
        let expected = count
            .checked_mul(8)
            .and_then(|len| 10usize.checked_add(len))
            .ok_or(ShortcutError::Overflow)?;
        if bytes.len() < expected {
            return Err(ShortcutError::Truncated);
        }
        if bytes.len() > expected {
            return Err(ShortcutError::TrailingBytes(bytes.len() - expected));
        }
        let mut bindings = Vec::with_capacity(count);
        for entry in bytes[10..].chunks_exact(8) {
            let modifiers = ShortcutModifiers(entry[0]);
            let raw_value = u32::from_le_bytes([entry[2], entry[3], entry[4], entry[5]]);
            let key = decode_key(entry[1], raw_value)?;
            let action =
                ToolbarAction::from_byte(entry[6]).ok_or(ShortcutError::UnknownAction(entry[6]))?;
            if entry[7] != 0 {
                return Err(ShortcutError::NonZeroReserved(entry[7]));
            }
            bindings.push(ShortcutBinding {
                gesture: ShortcutGesture { modifiers, key },
                action,
            });
        }
        let value = Self { bindings };
        value.validate()?;
        Ok(value)
    }
}

fn validate_gesture(gesture: ShortcutGesture) -> Result<(), ShortcutError> {
    if gesture.modifiers.0 & !ShortcutModifiers::KNOWN != 0 {
        return Err(ShortcutError::InvalidModifiers(gesture.modifiers.0));
    }
    match gesture.key {
        ShortcutKey::Character(value) if value.is_control() || value.is_whitespace() => {
            Err(ShortcutError::InvalidCharacter(u32::from(value)))
        }
        ShortcutKey::Function(value) if !(1..=24).contains(&value) => {
            Err(ShortcutError::InvalidFunctionKey(value))
        }
        _ => Ok(()),
    }
}

fn encode_key(key: ShortcutKey) -> (u8, u32) {
    match key {
        ShortcutKey::Character(value) => (0, u32::from(value)),
        ShortcutKey::Function(value) => (1, u32::from(value)),
        ShortcutKey::Backspace => (2, 0),
        ShortcutKey::Delete => (3, 0),
        ShortcutKey::Enter => (4, 0),
        ShortcutKey::Escape => (5, 0),
        ShortcutKey::ArrowLeft => (6, 0),
        ShortcutKey::ArrowRight => (7, 0),
        ShortcutKey::ArrowUp => (8, 0),
        ShortcutKey::ArrowDown => (9, 0),
        ShortcutKey::Insert => (10, 0),
        ShortcutKey::Home => (11, 0),
        ShortcutKey::End => (12, 0),
        ShortcutKey::PageUp => (13, 0),
        ShortcutKey::PageDown => (14, 0),
        ShortcutKey::Tab => (15, 0),
        ShortcutKey::Space => (16, 0),
        ShortcutKey::MouseLeft => (17, 0),
        ShortcutKey::MouseRight => (18, 0),
        ShortcutKey::MouseMiddle => (19, 0),
        ShortcutKey::MouseExtra1 => (20, 0),
        ShortcutKey::MouseExtra2 => (21, 0),
        ShortcutKey::Pause => (22, 0),
        ShortcutKey::NumpadMultiply => (23, 0),
        ShortcutKey::NumpadAdd => (24, 0),
        ShortcutKey::NumpadSeparator => (25, 0),
        ShortcutKey::NumpadSubtract => (26, 0),
        ShortcutKey::NumpadDecimal => (27, 0),
        ShortcutKey::NumpadDivide => (28, 0),
    }
}

fn decode_key(kind: u8, value: u32) -> Result<ShortcutKey, ShortcutError> {
    match kind {
        0 => char::from_u32(value)
            .map(ShortcutKey::Character)
            .ok_or(ShortcutError::InvalidCharacter(value)),
        1 => u8::try_from(value)
            .map(ShortcutKey::Function)
            .map_err(|_| ShortcutError::InvalidFunctionKey(u8::MAX)),
        2..=28 if value != 0 => Err(ShortcutError::UnknownKey(kind)),
        2 => Ok(ShortcutKey::Backspace),
        3 => Ok(ShortcutKey::Delete),
        4 => Ok(ShortcutKey::Enter),
        5 => Ok(ShortcutKey::Escape),
        6 => Ok(ShortcutKey::ArrowLeft),
        7 => Ok(ShortcutKey::ArrowRight),
        8 => Ok(ShortcutKey::ArrowUp),
        9 => Ok(ShortcutKey::ArrowDown),
        10 => Ok(ShortcutKey::Insert),
        11 => Ok(ShortcutKey::Home),
        12 => Ok(ShortcutKey::End),
        13 => Ok(ShortcutKey::PageUp),
        14 => Ok(ShortcutKey::PageDown),
        15 => Ok(ShortcutKey::Tab),
        16 => Ok(ShortcutKey::Space),
        17 => Ok(ShortcutKey::MouseLeft),
        18 => Ok(ShortcutKey::MouseRight),
        19 => Ok(ShortcutKey::MouseMiddle),
        20 => Ok(ShortcutKey::MouseExtra1),
        21 => Ok(ShortcutKey::MouseExtra2),
        22 => Ok(ShortcutKey::Pause),
        23 => Ok(ShortcutKey::NumpadMultiply),
        24 => Ok(ShortcutKey::NumpadAdd),
        25 => Ok(ShortcutKey::NumpadSeparator),
        26 => Ok(ShortcutKey::NumpadSubtract),
        27 => Ok(ShortcutKey::NumpadDecimal),
        28 => Ok(ShortcutKey::NumpadDivide),
        _ => Err(ShortcutError::UnknownKey(kind)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ShortcutConfig {
        ShortcutConfig {
            bindings: vec![
                ShortcutBinding {
                    gesture: ShortcutGesture {
                        modifiers: ShortcutModifiers::PRIMARY,
                        key: ShortcutKey::Character('s'),
                    },
                    action: ToolbarAction::Save,
                },
                ShortcutBinding {
                    gesture: ShortcutGesture {
                        modifiers: ShortcutModifiers::PRIMARY.union(ShortcutModifiers::SHIFT),
                        key: ShortcutKey::Character('s'),
                    },
                    action: ToolbarAction::SaveAs,
                },
            ],
        }
    }

    #[test]
    fn unicode_safe_logical_bindings_round_trip_and_resolve() {
        let mut expected = config();
        expected.bindings.push(ShortcutBinding {
            gesture: ShortcutGesture {
                modifiers: ShortcutModifiers::ALT,
                key: ShortcutKey::Character('月'),
            },
            action: ToolbarAction::ShowMap16,
        });
        let bytes = expected.encode().unwrap();
        let decoded = ShortcutConfig::decode(&bytes).unwrap();
        assert_eq!(decoded, expected);
        assert_eq!(decoded.encode().unwrap(), bytes);
        assert_eq!(
            decoded.action_for(expected.bindings[2].gesture),
            Some(ToolbarAction::ShowMap16)
        );
    }

    #[test]
    fn every_truncation_trailing_reserved_and_unknown_action_fails() {
        let bytes = config().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(ShortcutConfig::decode(&bytes[..end]).is_err());
        }
        let mut malformed = bytes.clone();
        malformed.push(0);
        assert_eq!(
            ShortcutConfig::decode(&malformed),
            Err(ShortcutError::TrailingBytes(1))
        );
        malformed = bytes.clone();
        malformed[17] = 1;
        assert_eq!(
            ShortcutConfig::decode(&malformed),
            Err(ShortcutError::NonZeroReserved(1))
        );
        malformed = bytes;
        malformed[16] = 0xff;
        assert_eq!(
            ShortcutConfig::decode(&malformed),
            Err(ShortcutError::UnknownAction(0xff))
        );
    }

    #[test]
    fn duplicate_and_invalid_gestures_are_rejected() {
        let mut duplicate = config();
        duplicate.bindings.push(duplicate.bindings[0]);
        assert!(matches!(
            duplicate.validate(),
            Err(ShortcutError::DuplicateGesture(_))
        ));
        for key in [ShortcutKey::Character('\n'), ShortcutKey::Function(25)] {
            assert!(
                ShortcutConfig {
                    bindings: vec![ShortcutBinding {
                        gesture: ShortcutGesture {
                            modifiers: ShortcutModifiers::default(),
                            key,
                        },
                        action: ToolbarAction::Open,
                    }],
                }
                .validate()
                .is_err()
            );
        }
    }

    #[test]
    fn appended_windows_navigation_keys_round_trip_without_renumbering_legacy_keys() {
        for key in [
            ShortcutKey::Insert,
            ShortcutKey::Home,
            ShortcutKey::End,
            ShortcutKey::PageUp,
            ShortcutKey::PageDown,
            ShortcutKey::Tab,
            ShortcutKey::Space,
        ] {
            let config = ShortcutConfig {
                bindings: vec![ShortcutBinding {
                    gesture: ShortcutGesture {
                        modifiers: ShortcutModifiers::SECONDARY,
                        key,
                    },
                    action: ToolbarAction::Open,
                }],
            };
            assert_eq!(
                ShortcutConfig::decode(&config.encode().unwrap()).unwrap(),
                config
            );
        }
        assert_eq!(encode_key(ShortcutKey::ArrowDown), (9, 0));
    }

    #[test]
    fn appended_mouse_buttons_round_trip_without_renumbering_keyboard_keys() {
        let keyboard = ShortcutConfig {
            bindings: vec![ShortcutBinding {
                gesture: ShortcutGesture {
                    modifiers: ShortcutModifiers::default(),
                    key: ShortcutKey::Space,
                },
                action: ToolbarAction::Open,
            }],
        }
        .encode()
        .unwrap();
        assert_eq!(keyboard[11], 16);

        for (key, kind) in [
            (ShortcutKey::MouseLeft, 17),
            (ShortcutKey::MouseRight, 18),
            (ShortcutKey::MouseMiddle, 19),
            (ShortcutKey::MouseExtra1, 20),
            (ShortcutKey::MouseExtra2, 21),
        ] {
            let expected = ShortcutConfig {
                bindings: vec![ShortcutBinding {
                    gesture: ShortcutGesture {
                        modifiers: ShortcutModifiers::ALT,
                        key,
                    },
                    action: ToolbarAction::ShowMap16,
                }],
            };
            let bytes = expected.encode().unwrap();
            assert_eq!(bytes[11], kind);
            assert_eq!(ShortcutConfig::decode(&bytes).unwrap(), expected);
        }
    }

    #[test]
    fn appended_pause_and_numpad_operators_round_trip_without_renumbering_prior_keys() {
        assert_eq!(encode_key(ShortcutKey::MouseExtra2), (21, 0));
        for (key, kind) in [
            (ShortcutKey::Pause, 22),
            (ShortcutKey::NumpadMultiply, 23),
            (ShortcutKey::NumpadAdd, 24),
            (ShortcutKey::NumpadSeparator, 25),
            (ShortcutKey::NumpadSubtract, 26),
            (ShortcutKey::NumpadDecimal, 27),
            (ShortcutKey::NumpadDivide, 28),
        ] {
            let expected = ShortcutConfig {
                bindings: vec![ShortcutBinding {
                    gesture: ShortcutGesture {
                        modifiers: ShortcutModifiers::SHIFT,
                        key,
                    },
                    action: ToolbarAction::ShowMap16,
                }],
            };
            let bytes = expected.encode().unwrap();
            assert_eq!(bytes[11], kind);
            assert_eq!(ShortcutConfig::decode(&bytes).unwrap(), expected);
        }
    }
}
