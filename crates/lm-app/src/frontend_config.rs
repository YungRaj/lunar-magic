//! Atomic aggregate configuration for toolkit-native frontends.

use crate::{
    LocalizationCatalog, LocalizationError, ShortcutConfig, ShortcutError, ToolbarConfig,
    ToolbarError,
};

const MAGIC: &[u8; 8] = b"LMUICFG1";
const SECTION_COUNT: usize = 3;
const HEADER_LEN: usize = MAGIC.len() + SECTION_COUNT * 4;
const MAX_SECTION_BYTES: usize = 0x10_0000;
const MAX_FILE_BYTES: usize = HEADER_LEN + SECTION_COUNT * MAX_SECTION_BYTES;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontendConfig {
    pub localization: LocalizationCatalog,
    pub toolbar: ToolbarConfig,
    pub shortcuts: ShortcutConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrontendConfigError {
    WrongMagic,
    Truncated,
    TrailingBytes(usize),
    FileTooLarge(usize),
    SectionTooLarge { section: usize, bytes: usize },
    Localization(LocalizationError),
    Toolbar(ToolbarError),
    Shortcuts(ShortcutError),
    Overflow,
}

impl std::fmt::Display for FrontendConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "frontend configuration bundle error: {self:?}")
    }
}

impl std::error::Error for FrontendConfigError {}

impl FrontendConfig {
    pub const MAX_ENCODED_LEN: usize = MAX_FILE_BYTES;

    /// Validates every component before any application state is replaced.
    ///
    /// # Errors
    ///
    /// Returns [`FrontendConfigError`] with the failing component.
    pub fn validate(&self) -> Result<(), FrontendConfigError> {
        self.localization
            .validate()
            .map_err(FrontendConfigError::Localization)?;
        self.toolbar
            .validate()
            .map_err(FrontendConfigError::Toolbar)?;
        self.shortcuts
            .validate()
            .map_err(FrontendConfigError::Shortcuts)?;
        Ok(())
    }

    /// Encodes all three canonical component formats inside `LMUICFG1`.
    ///
    /// # Errors
    ///
    /// Returns [`FrontendConfigError`] for invalid components or exceeded resource limits.
    pub fn encode(&self) -> Result<Vec<u8>, FrontendConfigError> {
        self.validate()?;
        let sections = [
            self.localization
                .encode()
                .map_err(FrontendConfigError::Localization)?,
            self.toolbar
                .encode()
                .map_err(FrontendConfigError::Toolbar)?,
            self.shortcuts
                .encode()
                .map_err(FrontendConfigError::Shortcuts)?,
        ];
        let total =
            sections
                .iter()
                .enumerate()
                .try_fold(HEADER_LEN, |total, (section, bytes)| {
                    if bytes.len() > MAX_SECTION_BYTES {
                        return Err(FrontendConfigError::SectionTooLarge {
                            section,
                            bytes: bytes.len(),
                        });
                    }
                    total
                        .checked_add(bytes.len())
                        .ok_or(FrontendConfigError::Overflow)
                })?;
        let mut output = Vec::with_capacity(total);
        output.extend_from_slice(MAGIC);
        for bytes in &sections {
            output.extend_from_slice(
                &u32::try_from(bytes.len())
                    .map_err(|_| FrontendConfigError::Overflow)?
                    .to_le_bytes(),
            );
        }
        for bytes in sections {
            output.extend_from_slice(&bytes);
        }
        Ok(output)
    }

    /// Decodes one complete bounded `LMUICFG1` bundle and validates every nested format.
    ///
    /// # Errors
    ///
    /// Returns [`FrontendConfigError`] for malformed framing, excessive sections, trailing data,
    /// arithmetic overflow, or an invalid nested component.
    pub fn decode(bytes: &[u8]) -> Result<Self, FrontendConfigError> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(FrontendConfigError::FileTooLarge(bytes.len()));
        }
        let header = bytes
            .get(..HEADER_LEN)
            .ok_or(FrontendConfigError::Truncated)?;
        if &header[..MAGIC.len()] != MAGIC {
            return Err(FrontendConfigError::WrongMagic);
        }
        let mut lengths = [0; SECTION_COUNT];
        for (section, length) in lengths.iter_mut().enumerate() {
            let offset = MAGIC.len() + section * 4;
            *length = usize::try_from(u32::from_le_bytes([
                header[offset],
                header[offset + 1],
                header[offset + 2],
                header[offset + 3],
            ]))
            .map_err(|_| FrontendConfigError::Overflow)?;
            if *length > MAX_SECTION_BYTES {
                return Err(FrontendConfigError::SectionTooLarge {
                    section,
                    bytes: *length,
                });
            }
        }
        let expected = lengths.iter().try_fold(HEADER_LEN, |total, length| {
            total
                .checked_add(*length)
                .ok_or(FrontendConfigError::Overflow)
        })?;
        if bytes.len() < expected {
            return Err(FrontendConfigError::Truncated);
        }
        if bytes.len() > expected {
            return Err(FrontendConfigError::TrailingBytes(bytes.len() - expected));
        }
        let mut offset = HEADER_LEN;
        let mut next = |length: usize| {
            let start = offset;
            offset += length;
            &bytes[start..offset]
        };
        let localization = LocalizationCatalog::decode(next(lengths[0]))
            .map_err(FrontendConfigError::Localization)?;
        let toolbar =
            ToolbarConfig::decode(next(lengths[1])).map_err(FrontendConfigError::Toolbar)?;
        let shortcuts =
            ShortcutConfig::decode(next(lengths[2])).map_err(FrontendConfigError::Shortcuts)?;
        Ok(Self {
            localization,
            toolbar,
            shortcuts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ShortcutBinding, ShortcutGesture, ShortcutKey, ShortcutModifiers, ToolbarAction,
        ToolbarItem, UiTextKey,
    };

    fn config() -> FrontendConfig {
        FrontendConfig {
            localization: LocalizationCatalog::new(
                "en-US",
                UiTextKey::ALL.map(|key| (key, format!("{key:?}"))),
            )
            .unwrap(),
            toolbar: ToolbarConfig {
                items: vec![ToolbarItem::Action {
                    id: "file.save".into(),
                    action: ToolbarAction::Save,
                    label: UiTextKey::FileSave,
                }],
            },
            shortcuts: ShortcutConfig {
                bindings: vec![ShortcutBinding {
                    gesture: ShortcutGesture {
                        modifiers: ShortcutModifiers::PRIMARY,
                        key: ShortcutKey::Character('s'),
                    },
                    action: ToolbarAction::Save,
                }],
            },
        }
    }

    #[test]
    fn complete_bundle_round_trips_canonically() {
        let expected = config();
        let bytes = expected.encode().unwrap();
        assert_eq!(FrontendConfig::decode(&bytes).unwrap(), expected);
        assert_eq!(
            FrontendConfig::decode(&bytes).unwrap().encode().unwrap(),
            bytes
        );
    }

    #[test]
    fn every_truncation_trailing_data_and_nested_corruption_fails() {
        let bytes = config().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(FrontendConfig::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            FrontendConfig::decode(&trailing),
            Err(FrontendConfigError::TrailingBytes(1))
        );
        let mut corrupt = bytes;
        corrupt[HEADER_LEN] ^= 0xff;
        assert!(matches!(
            FrontendConfig::decode(&corrupt),
            Err(FrontendConfigError::Localization(_))
        ));
    }
}
