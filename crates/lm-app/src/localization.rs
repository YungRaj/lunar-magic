//! Typed, toolkit-independent user-interface localization.

use std::collections::BTreeMap;

const MAGIC: &[u8; 8] = b"LMLOC001";
const MAX_LOCALE_BYTES: usize = 64;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_ENCODED_BYTES: usize =
    MAGIC.len() + 2 + MAX_LOCALE_BYTES + 2 + UiTextKey::ALL.len() * (1 + 2 + MAX_TEXT_BYTES);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum UiTextKey {
    AppTitle,
    FileOpen,
    FileSave,
    FileSaveAs,
    FileClose,
    FileQuit,
    EditUndo,
    EditRedo,
    EditCopy,
    EditCut,
    EditPaste,
    ViewLevel,
    ViewOverworld,
    ViewMap16,
    ViewGraphics,
    ViewPalette,
    ViewExAnimation,
    StatusReady,
    ViewLayer3,
}

impl UiTextKey {
    pub const ALL: [Self; 19] = [
        Self::AppTitle,
        Self::FileOpen,
        Self::FileSave,
        Self::FileSaveAs,
        Self::FileClose,
        Self::FileQuit,
        Self::EditUndo,
        Self::EditRedo,
        Self::EditCopy,
        Self::EditCut,
        Self::EditPaste,
        Self::ViewLevel,
        Self::ViewOverworld,
        Self::ViewMap16,
        Self::ViewGraphics,
        Self::ViewPalette,
        Self::ViewExAnimation,
        Self::StatusReady,
        Self::ViewLayer3,
    ];

    fn from_byte(value: u8) -> Option<Self> {
        Self::ALL.get(usize::from(value)).copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizationCatalog {
    pub locale: String,
    entries: BTreeMap<UiTextKey, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalizationError {
    WrongMagic,
    Truncated,
    TrailingBytes,
    InvalidUtf8,
    InvalidLocale,
    TextTooLong { key: UiTextKey, bytes: usize },
    InvalidText(UiTextKey),
    WrongEntryCount(usize),
    UnknownKey(u8),
    DuplicateKey(UiTextKey),
    MissingKey(UiTextKey),
    Overflow,
}

impl std::fmt::Display for LocalizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "localization catalog error: {self:?}")
    }
}

impl std::error::Error for LocalizationError {}

impl LocalizationCatalog {
    /// Maximum canonical `LMLOC001` size accepted by native bounded loaders.
    pub const MAX_ENCODED_LEN: usize = MAX_ENCODED_BYTES;

    #[must_use]
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Creates a complete catalog. Partial translations are rejected so frontends never need to
    /// guess whether an absent value is intentional.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] for invalid locale/text data or an incomplete/duplicate key
    /// set.
    pub fn new(
        locale: impl Into<String>,
        entries: impl IntoIterator<Item = (UiTextKey, String)>,
    ) -> Result<Self, LocalizationError> {
        let mut catalog = Self {
            locale: locale.into(),
            entries: BTreeMap::new(),
        };
        for (key, value) in entries {
            if catalog.entries.insert(key, value).is_some() {
                return Err(LocalizationError::DuplicateKey(key));
            }
        }
        catalog.validate()?;
        Ok(catalog)
    }

    #[must_use]
    /// Returns the translated value for a typed key.
    ///
    /// # Panics
    ///
    /// Panics only if an internal invariant is violated. Public constructors and decoding require
    /// every key and the entry map cannot be mutated externally.
    pub fn text(&self, key: UiTextKey) -> &str {
        self.entries
            .get(&key)
            .expect("validated catalogs contain every key")
    }

    /// Validates locale syntax, resource limits, and completeness.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] for invalid or incomplete catalog data.
    pub fn validate(&self) -> Result<(), LocalizationError> {
        if self.locale.is_empty()
            || self.locale.len() > MAX_LOCALE_BYTES
            || self
                .locale
                .bytes()
                .any(|byte| byte == 0 || byte.is_ascii_control())
        {
            return Err(LocalizationError::InvalidLocale);
        }
        for key in UiTextKey::ALL {
            let value = self
                .entries
                .get(&key)
                .ok_or(LocalizationError::MissingKey(key))?;
            if value.len() > MAX_TEXT_BYTES {
                return Err(LocalizationError::TextTooLong {
                    key,
                    bytes: value.len(),
                });
            }
            if value.is_empty() || value.contains('\0') {
                return Err(LocalizationError::InvalidText(key));
            }
        }
        if self.entries.len() != UiTextKey::ALL.len() {
            return Err(LocalizationError::WrongEntryCount(self.entries.len()));
        }
        Ok(())
    }

    /// Encodes the catalog canonically as `LMLOC001`.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] if the in-memory catalog is invalid or cannot be represented.
    pub fn encode(&self) -> Result<Vec<u8>, LocalizationError> {
        self.validate()?;
        let mut output = MAGIC.to_vec();
        write_string(&mut output, &self.locale, MAX_LOCALE_BYTES)?;
        let count = u16::try_from(UiTextKey::ALL.len()).map_err(|_| LocalizationError::Overflow)?;
        output.extend_from_slice(&count.to_le_bytes());
        for key in UiTextKey::ALL {
            output.push(key as u8);
            write_string(&mut output, self.text(key), MAX_TEXT_BYTES)?;
        }
        Ok(output)
    }

    /// Decodes one complete bounded `LMLOC001` catalog.
    ///
    /// # Errors
    ///
    /// Returns [`LocalizationError`] for malformed framing, invalid Unicode, unknown/duplicate
    /// keys, incomplete catalogs, or exceeded resource limits.
    pub fn decode(bytes: &[u8]) -> Result<Self, LocalizationError> {
        let mut reader = Reader::new(bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(LocalizationError::WrongMagic);
        }
        let locale = reader.string(MAX_LOCALE_BYTES)?;
        let count = usize::from(reader.u16()?);
        if count != UiTextKey::ALL.len() {
            return Err(LocalizationError::WrongEntryCount(count));
        }
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let raw = reader.byte()?;
            let key = UiTextKey::from_byte(raw).ok_or(LocalizationError::UnknownKey(raw))?;
            let value = reader.string(MAX_TEXT_BYTES)?;
            if entries.insert(key, value).is_some() {
                return Err(LocalizationError::DuplicateKey(key));
            }
        }
        if !reader.is_empty() {
            return Err(LocalizationError::TrailingBytes);
        }
        Self::new(locale, entries)
    }
}

fn write_string(output: &mut Vec<u8>, value: &str, limit: usize) -> Result<(), LocalizationError> {
    if value.len() > limit {
        return Err(LocalizationError::Overflow);
    }
    let len = u16::try_from(value.len()).map_err(|_| LocalizationError::Overflow)?;
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], LocalizationError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(LocalizationError::Overflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(LocalizationError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, LocalizationError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, LocalizationError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("length checked"),
        ))
    }

    fn string(&mut self, limit: usize) -> Result<String, LocalizationError> {
        let len = usize::from(self.u16()?);
        if len > limit {
            return Err(LocalizationError::Overflow);
        }
        std::str::from_utf8(self.take(len)?)
            .map(str::to_owned)
            .map_err(|_| LocalizationError::InvalidUtf8)
    }

    const fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> LocalizationCatalog {
        LocalizationCatalog::new(
            "fr-CA",
            UiTextKey::ALL.map(|key| (key, format!("texte-{key:?}"))),
        )
        .unwrap()
    }

    #[test]
    fn complete_unicode_catalog_round_trips_canonically() {
        let mut expected = catalog();
        expected
            .entries
            .insert(UiTextKey::AppTitle, "Éditeur 🌙".into());
        let bytes = expected.encode().unwrap();
        assert_eq!(LocalizationCatalog::decode(&bytes).unwrap(), expected);
        assert_eq!(
            LocalizationCatalog::decode(&bytes)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn published_encoded_limit_accepts_the_largest_valid_catalog() {
        let catalog = LocalizationCatalog::new(
            "l".repeat(MAX_LOCALE_BYTES),
            UiTextKey::ALL.map(|key| (key, "x".repeat(MAX_TEXT_BYTES))),
        )
        .unwrap();
        let bytes = catalog.encode().unwrap();
        assert_eq!(bytes.len(), LocalizationCatalog::MAX_ENCODED_LEN);
        assert_eq!(LocalizationCatalog::decode(&bytes).unwrap(), catalog);
    }

    #[test]
    fn every_truncation_trailing_byte_and_unknown_key_is_rejected() {
        let bytes = catalog().encode().unwrap();
        for end in 0..bytes.len() {
            assert!(LocalizationCatalog::decode(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            LocalizationCatalog::decode(&trailing),
            Err(LocalizationError::TrailingBytes)
        );
        let mut unknown = bytes;
        let first_key = MAGIC.len() + 2 + "fr-CA".len() + 2;
        unknown[first_key] = 0xff;
        assert_eq!(
            LocalizationCatalog::decode(&unknown),
            Err(LocalizationError::UnknownKey(0xff))
        );
    }

    #[test]
    fn missing_duplicate_and_invalid_values_fail_validation() {
        let mut entries = UiTextKey::ALL.map(|key| (key, format!("{key:?}"))).to_vec();
        entries.pop();
        assert!(matches!(
            LocalizationCatalog::new("en", entries),
            Err(LocalizationError::MissingKey(_))
        ));
        let mut duplicate = UiTextKey::ALL.map(|key| (key, format!("{key:?}"))).to_vec();
        duplicate.push((UiTextKey::AppTitle, "again".into()));
        assert_eq!(
            LocalizationCatalog::new("en", duplicate),
            Err(LocalizationError::DuplicateKey(UiTextKey::AppTitle))
        );
        assert!(matches!(
            LocalizationCatalog::new("", UiTextKey::ALL.map(|key| (key, "x".into()))),
            Err(LocalizationError::InvalidLocale)
        ));
    }
}
