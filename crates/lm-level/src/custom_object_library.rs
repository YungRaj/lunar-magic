use crate::ObjectRecord;
use std::fmt;

mod codec;

use codec::{
    decode_descriptions, decode_objects, encoded_data_len, encoded_description_len,
    validate_encoded_sizes,
};

pub const MAX_CUSTOM_OBJECT_SIDECAR_LEN: usize = 0x8000;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";

/// Text framing retained from a Lunar Magic `.mw0t` custom-object description sidecar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DescriptionFormat {
    pub utf8_bom: bool,
    pub line_ending: LineEnding,
    pub trailing_line_ending: bool,
}

impl Default for DescriptionFormat {
    fn default() -> Self {
        Self {
            utf8_bom: true,
            line_ending: LineEnding::CrLf,
            trailing_line_ending: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    pub(super) const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::CrLf => b"\r\n",
        }
    }
}

/// One synchronized entry from Lunar Magic's `.mw0` and `.mw0t` sidecars.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomObjectEntry {
    pub object: ObjectRecord,
    pub description: String,
}

impl CustomObjectEntry {
    /// Constructs an entry while enforcing the native text-sidecar boundary.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::InvalidDescription`] for embedded line separators,
    /// NULs, or descriptions longer than 1,024 encoded bytes.
    pub fn new(
        object: ObjectRecord,
        description: String,
    ) -> Result<Self, CustomObjectLibraryError> {
        validate_description(&description)?;
        Ok(Self {
            object,
            description,
        })
    }
}

/// Lossless paired custom-object library used by `.mw0` and `.mw0t` files.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CustomObjectLibrary {
    entries: Vec<CustomObjectEntry>,
    description_format: DescriptionFormat,
}

impl CustomObjectLibrary {
    /// Decodes a synchronized native sidecar pair.
    ///
    /// The binary sidecar is a terminated sequence of ordinary variable-width level-object
    /// records. The text sidecar is UTF-8 with an optional BOM and either consistent LF or CRLF
    /// separators. Text framing is retained for byte-stable re-encoding.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] for oversized, malformed, non-UTF-8, mismatched, or
    /// non-canonical sidecars.
    pub fn decode(data: &[u8], descriptions: &[u8]) -> Result<Self, CustomObjectLibraryError> {
        if data.len() > MAX_CUSTOM_OBJECT_SIDECAR_LEN {
            return Err(CustomObjectLibraryError::DataTooLarge);
        }
        if descriptions.len() > MAX_CUSTOM_OBJECT_SIDECAR_LEN {
            return Err(CustomObjectLibraryError::DescriptionsTooLarge);
        }

        let objects = decode_objects(data)?;
        let (text, utf8_bom) = descriptions
            .strip_prefix(UTF8_BOM)
            .map_or((descriptions, false), |text| (text, true));
        let text = std::str::from_utf8(text)
            .map_err(|_| CustomObjectLibraryError::InvalidDescriptionEncoding)?;
        let (description_values, line_ending, trailing_line_ending) =
            decode_descriptions(text, objects.len())?;
        if objects.len() != description_values.len() {
            return Err(CustomObjectLibraryError::EntryCountMismatch {
                objects: objects.len(),
                descriptions: description_values.len(),
            });
        }

        let entries = objects
            .into_iter()
            .zip(description_values)
            .map(|(object, description)| CustomObjectEntry::new(object, description))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            entries,
            description_format: DescriptionFormat {
                utf8_bom,
                line_ending,
                trailing_line_ending,
            },
        })
    }

    #[must_use]
    pub fn entries(&self) -> &[CustomObjectEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn description_format(&self) -> DescriptionFormat {
        self.description_format
    }

    /// Changes retained text framing after proving that the resulting sidecar remains bounded.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::DescriptionsTooLarge`] if the selected framing would
    /// exceed the native 32-KiB sidecar buffer.
    pub fn set_description_format(
        &mut self,
        format: DescriptionFormat,
    ) -> Result<(), CustomObjectLibraryError> {
        encoded_description_len(&self.entries, format)?;
        self.description_format = format;
        Ok(())
    }

    /// Appends a synchronized object and description atomically.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] if either resulting sidecar exceeds its native bound.
    pub fn push(&mut self, entry: CustomObjectEntry) -> Result<(), CustomObjectLibraryError> {
        self.insert(self.entries.len(), entry)
    }

    /// Inserts a synchronized entry atomically.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] for an invalid index or either size limit.
    pub fn insert(
        &mut self,
        index: usize,
        entry: CustomObjectEntry,
    ) -> Result<(), CustomObjectLibraryError> {
        if index > self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(index));
        }
        validate_description(&entry.description)?;
        let mut staged = self.entries.clone();
        staged.insert(index, entry);
        validate_encoded_sizes(&staged, self.description_format)?;
        self.entries = staged;
        Ok(())
    }

    /// Replaces a synchronized entry atomically.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] for an invalid index or either size limit.
    pub fn replace(
        &mut self,
        index: usize,
        entry: CustomObjectEntry,
    ) -> Result<CustomObjectEntry, CustomObjectLibraryError> {
        let Some(previous) = self.entries.get(index).cloned() else {
            return Err(CustomObjectLibraryError::InvalidIndex(index));
        };
        validate_description(&entry.description)?;
        let mut staged = self.entries.clone();
        staged[index] = entry;
        validate_encoded_sizes(&staged, self.description_format)?;
        self.entries = staged;
        Ok(previous)
    }

    /// Removes an object and its paired description.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::InvalidIndex`] for an absent entry.
    pub fn remove(&mut self, index: usize) -> Result<CustomObjectEntry, CustomObjectLibraryError> {
        if index >= self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(index));
        }
        Ok(self.entries.remove(index))
    }

    /// Moves an object and its description together.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError::InvalidIndex`] if either index is absent.
    pub fn move_entry(&mut self, from: usize, to: usize) -> Result<(), CustomObjectLibraryError> {
        if from >= self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(from));
        }
        if to >= self.entries.len() {
            return Err(CustomObjectLibraryError::InvalidIndex(to));
        }
        if from != to {
            let entry = self.entries.remove(from);
            self.entries.insert(to, entry);
        }
        Ok(())
    }

    /// Finds descriptions using Unicode lowercase matching while retaining library order.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query = query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                entry
                    .description
                    .to_lowercase()
                    .contains(&query)
                    .then_some(index)
            })
            .collect()
    }

    /// Encodes both synchronized sidecars with their retained text framing.
    ///
    /// # Errors
    ///
    /// Returns [`CustomObjectLibraryError`] if programmatic state exceeds either native bound.
    pub fn encode(&self) -> Result<(Vec<u8>, Vec<u8>), CustomObjectLibraryError> {
        validate_encoded_sizes(&self.entries, self.description_format)?;
        let mut data = Vec::with_capacity(encoded_data_len(&self.entries)?);
        for entry in &self.entries {
            data.extend_from_slice(entry.object.encoded());
        }
        data.push(0xff);

        let mut descriptions = Vec::with_capacity(encoded_description_len(
            &self.entries,
            self.description_format,
        )?);
        if self.description_format.utf8_bom {
            descriptions.extend_from_slice(UTF8_BOM);
        }
        for (index, entry) in self.entries.iter().enumerate() {
            descriptions.extend_from_slice(entry.description.as_bytes());
            if index + 1 < self.entries.len() || self.description_format.trailing_line_ending {
                descriptions.extend_from_slice(self.description_format.line_ending.bytes());
            }
        }
        Ok((data, descriptions))
    }
}

fn validate_description(description: &str) -> Result<(), CustomObjectLibraryError> {
    if description.len() > codec::MAX_DESCRIPTION_LEN || description.contains(['\0', '\r', '\n']) {
        Err(CustomObjectLibraryError::InvalidDescription)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomObjectLibraryError {
    DataTooLarge,
    DescriptionsTooLarge,
    MissingTerminator,
    TrailingObjectBytes(usize),
    MalformedObject { offset: usize },
    InvalidDescriptionEncoding,
    InvalidDescription,
    MixedLineEndings,
    EntryCountMismatch { objects: usize, descriptions: usize },
    InvalidIndex(usize),
}

impl fmt::Display for CustomObjectLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid custom-object sidecar library: {self:?}")
    }
}

impl std::error::Error for CustomObjectLibraryError {}

#[cfg(test)]
#[path = "custom_object_library_tests.rs"]
mod tests;
